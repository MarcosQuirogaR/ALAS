// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Packaging and distribution assembly task for xtask.
//!
//! The distribution is intentionally assembled from a release binary plus
//! explicitly allowlisted adjacent tools. This module also writes machine
//! readable provenance so a package cannot imply that an optional solver or
//! source snapshot was included when the staging directory was incomplete.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
const NASTRAN_STRICT_ENV: &str = "ALAS_STRICT_BUNDLED_NASTRAN95";
const SOURCE_MANIFEST_NAME: &str = "SOURCE-MANIFEST.json";
const RELEASE_MANIFEST_NAME: &str = "RELEASE-MANIFEST.json";
const SOURCE_DIRECTORY_ROOTS: [&str; 5] = ["crates", "xtask", "tools", "docs", ".cargo"];
const SOURCE_ROOT_FILES: [&str; 12] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "deny.toml",
    "README.md",
    "LICENSE",
    "NOTICE",
    "THIRD-PARTY-NOTICES.md",
    "CONTRIBUTING.md",
    ".gitattributes",
    "app_icon.ico",
];
const SOURCE_BRANDING_FILES: [&str; 3] = ["app_logo.png", "app_text_logo.png", "app_branding.png"];
const SOURCE_DOCUMENT_EXTENSIONS: [&str; 7] = ["md", "json", "toml", "yaml", "yml", "tsv", "txt"];
const SOURCE_TOOL_EXTENSIONS: [&str; 10] = [
    "cjs", "js", "mjs", "py", "ps1", "toml", "json", "md", "txt", "tsv",
];

// These directories contain data consumed by production code or by offline
// tests during compilation. They are named explicitly so an unrelated data
// dump added to a crate cannot enter a public source archive just because it
// has a convenient extension.
const SOURCE_DATA_DIRECTORY_PREFIXES: [&str; 6] = [
    "crates/alas-aero/data",
    "crates/alas-config/data",
    "crates/alas-geom/data",
    "crates/alas-i18n/data",
    "crates/alas-uav/data",
    "crates/alas-uav/examples/data",
];
const SOURCE_TEST_FIXTURE_DIRECTORY_PREFIXES: [&str; 2] = [
    "crates/alas-aero/tests/fixtures",
    "crates/alas-uav/tests/fixtures",
];

// Most golden evidence remains local to the development checkout. These
// files are the narrow exception: current tests use include_str!/include_bytes!
// with hard-coded paths, so omitting them makes an unpacked source snapshot
// fail at compile time. Keep the list exact and add entries only with a test
// consumer and a provenance decision.
const SOURCE_GOLDEN_FIXTURE_FILES: [&str; 8] = [
    "golden/report/reference_render_corpus.json",
    "golden/report/reference_render_w32.json",
    "golden/report/reference_render_w33.json",
    "golden/report/reference_render_w38.json",
    "golden/report/reference_render_w39.json",
    "golden/prop_elec/apc_12x6e_fixture.json",
    "golden/prop_elec/apc_performance_tables.bin",
    "golden/prop_elec/uiuc_apc_12x6_static_benchmark.json",
];

// The real-aircraft parity contract is the one exception admitted from
// otherwise-excluded `golden/` evidence, not a directory-prefix rule. It was
// read in full and reviewed for this entry: every numeric anchor is a short
// factual value (dimension, mass, count) with a `cite` key into a `sources`
// bibliography whose entries carry only `title`/`authority`/`url` -- no
// excerpted, quoted or reproduced manufacturer document text, no images, no
// secrets. `tools/aircraft_parity.cjs`, itself part of this source snapshot,
// requires this exact file to run at all. Add a sibling file here only after
// the same per-file review; do not widen this into a `golden/aircraft`
// prefix rule.
const SOURCE_REVIEWED_EVIDENCE_FILES: [&str; 1] = ["golden/aircraft/real_aircraft_parity.json"];

const SOURCE_REJECTED_PATH_COMPONENTS: [&str; 7] = [
    ".agent", "target", "dist", "outputs", "runs", "private", "secrets",
];
const SOURCE_REJECTED_FILE_PREFIXES: [&str; 5] =
    [".env", "credentials", "secret", "private", "id_rsa"];

#[derive(Debug, Clone)]
struct BundleStatus {
    status: &'static str,
    reason: Option<String>,
}

impl BundleStatus {
    fn bundled() -> Self {
        Self {
            status: "bundled",
            reason: None,
        }
    }

    fn not_bundled(reason: impl Into<String>) -> Self {
        Self {
            status: "not_bundled",
            reason: Some(reason.into()),
        }
    }

    /// A proprietary or process-boundary tool this task never attempts to
    /// bundle, distinct from [`Self::not_bundled`]'s "staging was incomplete"
    /// meaning. Recording it here keeps the manifest's tool inventory
    /// complete instead of silently naming only the two tools this task can
    /// bundle.
    fn user_supplied(reason: impl Into<String>) -> Self {
        Self {
            status: "user_supplied",
            reason: Some(reason.into()),
        }
    }
}

/// External solvers this task never bundles: proprietary licensing or a
/// user-selected external release makes redistribution the user's decision,
/// not this packaging task's. Listed in the release manifest so a reviewer
/// sees the complete required/optional tool inventory in one place instead of
/// only the two tools [`bundle_avl`]/[`bundle_nastran95`] can bundle. Full
/// licence and provenance detail lives in `THIRD-PARTY-NOTICES.md`, which
/// travels with the package.
const USER_SUPPLIED_EXTERNAL_TOOLS: [(&str, &str); 5] = [
    (
        "mses",
        "Proprietary MSES (mset/mses/mplot), per-seat licence from MIT; never bundled. See THIRD-PARTY-NOTICES.md.",
    ),
    (
        "vspaero",
        "OpenVSP/VSPAERO, NASA Open Source Agreement as supplied by the user-selected OpenVSP release; never bundled. See THIRD-PARTY-NOTICES.md.",
    ),
    (
        "msc_nastran",
        "Proprietary MSC Nastran; never bundled. See THIRD-PARTY-NOTICES.md.",
    ),
    (
        "msc_patran",
        "Proprietary MSC Patran; never bundled. See THIRD-PARTY-NOTICES.md.",
    ),
    (
        "flowunsteady",
        "User-supplied FLOWUnsteady adapter/Julia environment; licence follows the selected external release; never bundled. See THIRD-PARTY-NOTICES.md.",
    ),
];

#[derive(Debug, Clone)]
struct SourceRecord {
    path: String,
    bytes: usize,
    sha256: String,
}

#[derive(Debug, Clone)]
struct SourceSnapshot {
    records: Vec<SourceRecord>,
    excluded_count: usize,
}

#[derive(Debug, Clone)]
struct ArtifactRecord {
    path: String,
    bytes: usize,
    sha256: String,
}

/// Build and package the release distribution archive.
pub fn create_distribution(root: &Path) -> Result<(), String> {
    println!("Creating standalone release distribution for ALAS...\n");

    let strict_nastran = env_flag(NASTRAN_STRICT_ENV);
    if strict_nastran {
        println!(
            "Strict NASTRAN-95 bundle mode enabled via {NASTRAN_STRICT_ENV}=1; incomplete staging will fail"
        );
    } else {
        println!(
            "NASTRAN-95 bundle mode is optional; incomplete staging will be recorded as not_bundled"
        );
    }

    // 1. Compile optimized release binary.
    println!("Compiling release binary...");
    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .args(["build", "--release", "--locked", "--bin", "alas"])
        .status()
        .map_err(|e| format!("failed to compile release binary: {e}"))?;

    if !status.success() {
        return Err("release build failed".to_owned());
    }

    let exe_name = if cfg!(windows) { "alas.exe" } else { "alas" };
    let src_exe = cargo_target_dir(root).join("release").join(exe_name);
    if !src_exe.exists() {
        return Err(format!("release binary not found at {}", src_exe.display()));
    }

    // 2. Assemble distribution directory structure.
    let package_name = package_name();
    let dist_root = root.join("dist");
    let pkg_dir = dist_root.join(&package_name);

    if pkg_dir.exists() {
        fs::remove_dir_all(&pkg_dir)
            .map_err(|e| format!("failed to clean {}: {e}", pkg_dir.display()))?;
    }
    fs::create_dir_all(&pkg_dir)
        .map_err(|e| format!("failed to create {}: {e}", pkg_dir.display()))?;

    // Copy executable.
    let dst_exe = pkg_dir.join(exe_name);
    fs::copy(&src_exe, &dst_exe)
        .map_err(|e| format!("failed to copy binary to {}: {e}", dst_exe.display()))?;
    println!("Copied standalone binary: {}", dst_exe.display());

    let avl_status = bundle_avl(root, &pkg_dir)?;
    let nastran_status = bundle_nastran95(root, &pkg_dir, strict_nastran)?;
    bundle_branding(root, &pkg_dir)?;

    // Copy documentation and notices.
    for doc in ["README.md", "LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.md"] {
        let src_doc = root.join(doc);
        if src_doc.exists() {
            let dst_doc = pkg_dir.join(doc);
            fs::copy(&src_doc, &dst_doc)
                .map_err(|e| format!("failed to copy {doc} to {}: {e}", dst_doc.display()))?;
        }
    }

    // The AGPL source corresponding to the binary is copied from an explicit
    // git-tracked allowlist. Proprietary external tools and local evidence
    // are excluded before any package manifest is written.
    let source_snapshot = bundle_source_snapshot(root, &pkg_dir)?;

    // 3. Export configuration templates.
    let configs_dir = pkg_dir.join("configs");
    fs::create_dir_all(&configs_dir)
        .map_err(|e| format!("failed to create {}: {e}", configs_dir.display()))?;

    let sample_config = configs_dir.join("ave.yaml");
    run_save_config(&dst_exe, &sample_config)?;
    println!(
        "Generated configuration templates in {}",
        configs_dir.display()
    );

    // The release manifest is deliberately generated before validation and
    // archiving. Its artifact list excludes itself to avoid a self-referential
    // hash while still covering every shipped file.
    let release_manifest = pkg_dir.join(RELEASE_MANIFEST_NAME);
    write_release_manifest(
        root,
        &pkg_dir,
        &package_name,
        &source_snapshot,
        &avl_status,
        &nastran_status,
        &release_manifest,
    )?;

    // 4. Validate the packaged standalone distribution.
    println!("\nValidating packaged standalone distribution...");
    validate_distribution(&dst_exe, &sample_config, &release_manifest, &dist_root)?;

    // 5. Create distribution zip archive only after the package has passed its
    // executable, config, AVL, path-space, and provenance checks.
    let zip_name = format!("{package_name}.zip");
    let zip_path = dist_root.join(&zip_name);
    if zip_path.exists() {
        let _ = fs::remove_file(&zip_path);
    }

    println!("Creating package archive: {}", zip_path.display());
    create_zip_archive(&dist_root, &package_name, &zip_name)?;
    let archive_bytes = fs::read(&zip_path).map_err(|e| {
        format!(
            "failed to read created package archive {}: {e}",
            zip_path.display()
        )
    })?;
    let checksum_path = dist_root.join(format!("{zip_name}.sha256"));
    fs::write(
        &checksum_path,
        format!("{}  {zip_name}\n", sha256_hex(&archive_bytes)),
    )
    .map_err(|e| {
        format!(
            "failed to write archive checksum {}: {e}",
            checksum_path.display()
        )
    })?;
    println!("Wrote package archive SHA-256: {}", checksum_path.display());

    println!("\nDistribution packaging complete: {}", zip_path.display());
    Ok(())
}

fn env_flag(name: &str) -> bool {
    env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn target_label() -> String {
    format!("{}-{}", env::consts::OS, env::consts::ARCH)
}

fn package_name() -> String {
    format!("alas-v{PACKAGE_VERSION}-{}", target_label())
}

/// Copy the exact native branding sources and the transparent, square ICO
/// derivative used by Windows shells into the standalone package.
fn bundle_branding(root: &Path, package_dir: &Path) -> Result<(), String> {
    for name in [
        "app_logo.png",
        "app_text_logo.png",
        "app_branding.png",
        "app_icon.ico",
    ] {
        let source = root.join(name);
        if !source.is_file() {
            return Err(format!(
                "native branding artifact is missing: {}; release packaging requires the exact source artwork and app icon",
                source.display()
            ));
        }
        let destination = package_dir.join(name);
        fs::copy(&source, &destination).map_err(|e| {
            format!(
                "failed to copy native branding artifact to {}: {e}",
                destination.display()
            )
        })?;
    }
    println!("Bundled native ALAS branding artwork and Windows icon");
    Ok(())
}

/// Run the packaged binary's configuration exporter and verify a non-empty
/// file was created. Keeping this in one helper makes the same operation
/// usable for the package's portable path-space round-trip check.
fn run_save_config(exe: &Path, destination: &Path) -> Result<(), String> {
    let destination_text = path_arg(destination)?;
    let status = Command::new(exe)
        .args(["--save-config", &destination_text])
        .status()
        .map_err(|e| {
            format!(
                "failed to export configuration to {}: {e}",
                destination.display()
            )
        })?;

    if !status.success() {
        return Err(format!(
            "packaged binary failed to export configuration to {}",
            destination.display()
        ));
    }
    let metadata = fs::metadata(destination).map_err(|e| {
        format!(
            "configuration exporter did not create {}: {e}",
            destination.display()
        )
    })?;
    if metadata.len() == 0 {
        return Err(format!(
            "configuration exporter created an empty file at {}",
            destination.display()
        ));
    }
    Ok(())
}

fn path_arg(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

/// Copy the positive ALAS build closure through an explicit allowlist.
/// This source snapshot is meant to correspond to the built worktree, including
/// tracked and unignored source edits, while excluding proprietary tools, local
/// evidence, generated outputs, and data outside the declared closure.
fn bundle_source_snapshot(root: &Path, package_dir: &Path) -> Result<SourceSnapshot, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()
        .map_err(|e| format!("failed to enumerate the allowlisted git source snapshot: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed while preparing the source snapshot (status {})",
            output.status
        ));
    }

    let mut tracked = Vec::new();
    for item in output.stdout.split(|byte| *byte == 0) {
        if item.is_empty() {
            continue;
        }
        let path = String::from_utf8(item.to_vec())
            .map_err(|_| "git source snapshot contains a non-UTF-8 path".to_owned())?;
        tracked.push(path);
    }
    tracked.sort_unstable();

    let source_root = package_dir.join("source").join("alas");
    let canonical_root = fs::canonicalize(root)
        .map_err(|e| format!("failed to resolve repository root for source safety: {e}"))?;
    let mut records = Vec::new();
    let mut excluded_count = 0;
    for relative in tracked {
        if !source_path_allowed(&relative) {
            excluded_count += 1;
            continue;
        }

        let source = root.join(&relative);
        let canonical_source = fs::canonicalize(&source).map_err(|e| {
            format!(
                "failed to resolve allowlisted source path {}: {e}",
                source.display()
            )
        })?;
        if !canonical_source.starts_with(&canonical_root) {
            return Err(format!(
                "allowlisted source path resolves outside repository root: {}",
                source.display()
            ));
        }
        if !source.is_file() {
            return Err(format!(
                "allowlisted source path is not a regular file: {}",
                source.display()
            ));
        }
        let bytes = fs::read(&source).map_err(|e| {
            format!(
                "failed to read source snapshot file {}: {e}",
                source.display()
            )
        })?;
        let normalized = normalize_relative_path(&relative);
        let destination = source_root.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "failed to create source snapshot directory {}: {e}",
                    parent.display()
                )
            })?;
        }
        fs::write(&destination, &bytes).map_err(|e| {
            format!(
                "failed to write source snapshot file {}: {e}",
                destination.display()
            )
        })?;
        records.push(SourceRecord {
            path: normalized,
            bytes: bytes.len(),
            sha256: sha256_hex(&bytes),
        });
    }

    if records.is_empty() {
        return Err("allowlisted source snapshot is empty after the release allowlist".to_owned());
    }

    let snapshot = SourceSnapshot {
        records,
        excluded_count,
    };
    write_source_manifest(&source_root, &snapshot)?;
    println!(
        "Included {} allowlisted source files ({} excluded by release allowlist)",
        snapshot.records.len(),
        snapshot.excluded_count
    );
    Ok(snapshot)
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn source_path_allowed(path: &str) -> bool {
    let normalized = normalize_relative_path(path);
    if !is_safe_relative_path(&normalized) {
        return false;
    }

    if has_rejected_path_component(&normalized) {
        return false;
    }

    if SOURCE_ROOT_FILES
        .iter()
        .any(|candidate| *candidate == normalized)
        || SOURCE_BRANDING_FILES
            .iter()
            .any(|candidate| *candidate == normalized)
        || normalized == "assets/textures/earth_blue_marble.png"
        || SOURCE_GOLDEN_FIXTURE_FILES
            .iter()
            .any(|candidate| *candidate == normalized)
        || SOURCE_REVIEWED_EVIDENCE_FILES
            .iter()
            .any(|candidate| *candidate == normalized)
    {
        return true;
    }

    let mut components = normalized.split('/');
    let root = components.next().unwrap_or_default();
    let remainder = components.collect::<Vec<_>>();
    if remainder.is_empty() || !SOURCE_DIRECTORY_ROOTS.contains(&root) {
        return false;
    }

    let extension = Path::new(&normalized)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();

    match root {
        // Rust source and each crate manifest are required for a source-only
        // build. Non-source files under a crate must be admitted through an
        // explicit data/fixture prefix above.
        "crates" => {
            let is_allowlisted_data = SOURCE_DATA_DIRECTORY_PREFIXES
                .iter()
                .chain(SOURCE_TEST_FIXTURE_DIRECTORY_PREFIXES.iter())
                .any(|prefix| is_path_or_descendant(&normalized, prefix));
            is_allowlisted_data || extension.as_deref() == Some("rs") || file_name == "Cargo.toml"
        }
        "xtask" => extension.as_deref() == Some("rs") || file_name == "Cargo.toml",
        // The utility tree contains allowlisted source scripts as well as the
        // text/data documents accepted by the narrower documentation rule.
        // Binary outputs, archives, images, and arbitrary local files remain
        // excluded by this extension list.
        "tools" => extension.is_some_and(|value| {
            SOURCE_TOOL_EXTENSIONS
                .iter()
                .any(|candidate| *candidate == value)
        }),
        "docs" | ".cargo" => extension.is_some_and(|value| {
            SOURCE_DOCUMENT_EXTENSIONS
                .iter()
                .any(|candidate| *candidate == value)
        }),
        _ => false,
    }
}

fn is_path_or_descendant(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|remainder| remainder.starts_with('/'))
}

fn has_rejected_path_component(path: &str) -> bool {
    let mut components = path.split('/');
    components
        .clone()
        .any(|component| SOURCE_REJECTED_PATH_COMPONENTS.contains(&component))
        || components.any(|component| {
            let lower = component.to_ascii_lowercase();
            SOURCE_REJECTED_FILE_PREFIXES
                .iter()
                .any(|candidate| lower.starts_with(candidate))
        })
}

fn is_safe_relative_path(path: &str) -> bool {
    let candidate = Path::new(path);
    if candidate.is_absolute() || path.is_empty() {
        return false;
    }
    candidate.components().all(|component| {
        !matches!(
            component,
            std::path::Component::Prefix(_)
                | std::path::Component::RootDir
                | std::path::Component::ParentDir
                | std::path::Component::CurDir
        )
    })
}

fn write_source_manifest(source_root: &Path, snapshot: &SourceSnapshot) -> Result<(), String> {
    let manifest = source_root
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "source snapshot has no package parent".to_owned())?
        .join(SOURCE_MANIFEST_NAME);
    let mut json = String::from("{\n");
    json.push_str("  \"schema_version\": 1,\n");
    json.push_str("  \"selection\": \"positive-allowlisted-build-closure\",\n");
    json.push_str("  \"source_root\": \"source/alas\",\n");
    json.push_str(&format!(
        "  \"allowed_directory_roots\": {},\n",
        json_string_array(&SOURCE_DIRECTORY_ROOTS)
    ));
    json.push_str(&format!(
        "  \"allowed_root_files\": {},\n",
        json_string_array(&SOURCE_ROOT_FILES)
    ));
    json.push_str(&format!(
        "  \"allowed_branding_files\": {},\n",
        json_string_array(&SOURCE_BRANDING_FILES)
    ));
    json.push_str("  \"allowed_asset_files\": [\"assets/textures/earth_blue_marble.png\"],\n");
    json.push_str(&format!(
        "  \"allowed_data_directory_prefixes\": {},\n",
        json_string_array(&SOURCE_DATA_DIRECTORY_PREFIXES)
    ));
    json.push_str(&format!(
        "  \"allowed_test_fixture_directory_prefixes\": {},\n",
        json_string_array(&SOURCE_TEST_FIXTURE_DIRECTORY_PREFIXES)
    ));
    json.push_str(&format!(
        "  \"allowed_golden_fixture_files\": {},\n",
        json_string_array(&SOURCE_GOLDEN_FIXTURE_FILES)
    ));
    json.push_str(&format!(
        "  \"allowed_reviewed_evidence_files\": {},\n",
        json_string_array(&SOURCE_REVIEWED_EVIDENCE_FILES)
    ));
    json.push_str(&format!(
        "  \"allowed_tool_extensions\": {},\n",
        json_string_array(&SOURCE_TOOL_EXTENSIONS)
    ));
    json.push_str(&format!(
        "  \"excluded_file_count\": {},\n  \"files\": [\n",
        snapshot.excluded_count
    ));
    for (index, record) in snapshot.records.iter().enumerate() {
        json.push_str("    {\n");
        json.push_str(&format!("      \"path\": {},\n", json_string(&record.path)));
        json.push_str(&format!("      \"bytes\": {},\n", record.bytes));
        json.push_str(&format!(
            "      \"sha256\": {}\n",
            json_string(&record.sha256)
        ));
        json.push_str("    }");
        if index + 1 != snapshot.records.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  ]\n}\n");
    fs::write(&manifest, json).map_err(|e| format!("failed to write {}: {e}", manifest.display()))
}

fn json_string_array(values: &[&str]) -> String {
    let mut json = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            json.push_str(", ");
        }
        json.push_str(&json_string(value));
    }
    json.push(']');
    json
}

fn write_release_manifest(
    root: &Path,
    package_dir: &Path,
    package_name: &str,
    source_snapshot: &SourceSnapshot,
    avl_status: &BundleStatus,
    nastran_status: &BundleStatus,
    destination: &Path,
) -> Result<(), String> {
    let artifacts = collect_artifacts(package_dir)?;
    let commit = git_output(root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let dirty =
        git_output(root, &["status", "--porcelain"]).is_some_and(|text| !text.trim().is_empty());
    // The release command is run only for an explicitly selected package.
    // A plain semantic version is therefore the GA channel; pre-release
    // versions retain their specific channel for review tooling.
    let release_channel = if PACKAGE_VERSION.contains("-rc.") {
        "release_candidate"
    } else if PACKAGE_VERSION.contains('-') {
        "pre_release"
    } else {
        "release"
    };

    let mut json = String::from("{\n");
    json.push_str("  \"schema_version\": 1,\n");
    json.push_str("  \"product\": \"ALAS\",\n");
    json.push_str(&format!(
        "  \"version\": {},\n",
        json_string(PACKAGE_VERSION)
    ));
    json.push_str(&format!(
        "  \"release_channel\": {},\n",
        json_string(release_channel)
    ));
    json.push_str(&format!(
        "  \"package_name\": {},\n",
        json_string(package_name)
    ));
    json.push_str(&format!(
        "  \"target\": {},\n",
        json_string(&target_label())
    ));
    json.push_str("  \"profile\": \"release\",\n");
    json.push_str(&format!("  \"source_commit\": {},\n", json_string(&commit)));
    json.push_str(&format!("  \"working_tree_dirty\": {dirty},\n"));
    json.push_str("  \"source_snapshot\": {\n");
    json.push_str(&format!(
        "    \"status\": \"included\",\n    \"manifest\": {},\n    \"file_count\": {},\n    \"excluded_file_count\": {}\n",
        json_string(SOURCE_MANIFEST_NAME),
        source_snapshot.records.len(),
        source_snapshot.excluded_count
    ));
    json.push_str("  },\n");
    json.push_str("  \"external_tools\": {\n");
    json.push_str(&format!(
        "    \"avl\": {},\n",
        bundle_status_json(avl_status)
    ));
    json.push_str(&format!(
        "    \"nastran95\": {},\n",
        bundle_status_json(nastran_status)
    ));
    for (index, (name, reason)) in USER_SUPPLIED_EXTERNAL_TOOLS.iter().enumerate() {
        json.push_str(&format!(
            "    {}: {}",
            json_string(name),
            bundle_status_json(&BundleStatus::user_supplied(*reason))
        ));
        json.push_str(if index + 1 == USER_SUPPLIED_EXTERNAL_TOOLS.len() {
            "\n"
        } else {
            ",\n"
        });
    }
    json.push_str("  },\n");
    json.push_str("  \"artifacts\": [\n");
    for (index, artifact) in artifacts.iter().enumerate() {
        json.push_str("    {\n");
        json.push_str(&format!(
            "      \"path\": {},\n",
            json_string(&artifact.path)
        ));
        json.push_str(&format!("      \"bytes\": {},\n", artifact.bytes));
        json.push_str(&format!(
            "      \"sha256\": {}\n",
            json_string(&artifact.sha256)
        ));
        json.push_str("    }");
        if index + 1 != artifacts.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push_str("  ]\n}\n");
    fs::write(destination, json)
        .map_err(|e| format!("failed to write {}: {e}", destination.display()))
}

fn bundle_status_json(status: &BundleStatus) -> String {
    let mut json = format!("{{\"status\": {}", json_string(status.status));
    if let Some(reason) = &status.reason {
        json.push_str(&format!(", \"reason\": {}", json_string(reason)));
    }
    json.push('}');
    json
}

fn collect_artifacts(package_dir: &Path) -> Result<Vec<ArtifactRecord>, String> {
    let mut paths = Vec::new();
    collect_files_recursive(package_dir, package_dir, &mut paths)?;
    paths.sort();

    let mut artifacts = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = fs::read(&path)
            .map_err(|e| format!("failed to hash packaged artifact {}: {e}", path.display()))?;
        let relative = path
            .strip_prefix(package_dir)
            .map_err(|e| format!("failed to relativize packaged artifact: {e}"))?;
        artifacts.push(ArtifactRecord {
            path: normalize_relative_path(&relative.to_string_lossy()),
            bytes: bytes.len(),
            sha256: sha256_hex(&bytes),
        });
    }
    Ok(artifacts)
}

fn collect_files_recursive(
    root: &Path,
    directory: &Path,
    output: &mut Vec<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|e| format!("failed to enumerate {}: {e}", directory.display()))?
    {
        let entry = entry.map_err(|e| format!("failed to read package entry: {e}"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("failed to inspect {}: {e}", path.display()))?;
        if file_type.is_dir() {
            collect_files_recursive(root, &path, output)?;
        } else if file_type.is_file() && path != root.join(RELEASE_MANIFEST_NAME) {
            output.push(path);
        }
    }
    Ok(())
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

/// Copy the unchanged GPL AVL program and its corresponding source into the
/// distribution as an adjacent child executable.
///
/// Keeping this process boundary is what lets the AGPL application and the
/// GPL solver remain separately licensed works. The source archive and the
/// full GPL text travel with the exact executable used by the package.
fn bundle_avl(root: &Path, package_dir: &Path) -> Result<BundleStatus, String> {
    let source_dir = root.join("external tools");
    let package_tools = package_dir.join("external tools");
    fs::create_dir_all(&package_tools)
        .map_err(|e| format!("failed to create {}: {e}", package_tools.display()))?;

    for name in ["avl352.exe", "avl3.52.tgz", "AVL-GPL-2.0.txt"] {
        let source = source_dir.join(name);
        if !source.is_file() {
            return Err(format!(
                "bundled AVL artifact is missing: {}; release packaging requires the unchanged executable, source archive, and GPL text",
                source.display()
            ));
        }
        let destination = package_tools.join(name);
        fs::copy(&source, &destination).map_err(|e| {
            format!(
                "failed to copy bundled AVL artifact to {}: {e}",
                destination.display()
            )
        })?;
    }
    println!(
        "Bundled AVL 3.52 child executable, corresponding source, and GPL text in {}",
        package_tools.display()
    );
    Ok(BundleStatus::bundled())
}

/// Copy NASTRAN-95 as a separately licensed adjacent program when its
/// executable, runtime dependency declaration, license, modification record,
/// and source association are all present and internally checkable.
///
/// The default mode omits incomplete staging and records the reason in the
/// release manifest. Set `ALAS_STRICT_BUNDLED_NASTRAN95=1` to make an
/// intentional bundled release fail closed instead.
fn bundle_nastran95(root: &Path, package_dir: &Path, strict: bool) -> Result<BundleStatus, String> {
    let source = root.join("external tools").join("NASTRAN-95");
    let issues = assess_nastran95(&source);
    if !issues.is_empty() {
        let reason = issues.join("; ");
        if strict {
            return Err(format!(
                "strict NASTRAN-95 bundle mode refused incomplete staging at {}: {}",
                source.display(),
                reason
            ));
        }
        println!("[INFO] NASTRAN-95 not bundled: {reason}");
        return Ok(BundleStatus::not_bundled(reason));
    }

    let destination = package_dir.join("external tools").join("NASTRAN-95");
    copy_directory(&source, &destination)?;
    println!(
        "Bundled separately licensed NASTRAN-95 executable, runtime, rigid formats, corresponding source, and notices in {}",
        destination.display()
    );
    Ok(BundleStatus::bundled())
}

fn assess_nastran95(source: &Path) -> Vec<String> {
    let required_files = [
        "build/bin/nastran.exe",
        "build/bin/nastran95-build.txt",
        "rf/NASINFO",
        "LICENSE",
        "MODIFICATIONS.md",
        "SOURCE-REVISION.txt",
        "source/NASTRAN-95-source.zip",
    ];
    let mut issues = Vec::new();
    if !source.is_dir() {
        issues.push(format!(
            "staging directory is absent ({})",
            source.display()
        ));
        return issues;
    }
    for relative in required_files {
        let artifact = source.join(relative);
        if !artifact.is_file() {
            issues.push(format!("missing {relative}"));
        } else if fs::metadata(&artifact).is_ok_and(|metadata| metadata.len() == 0) {
            issues.push(format!("empty {relative}"));
        }
    }

    let runtime = source.join("runtime");
    if !runtime.is_dir() {
        issues.push("runtime directory is absent".to_owned());
    } else {
        let runtime_files = match fs::read_dir(&runtime) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
                .collect::<Vec<_>>(),
            Err(error) => {
                issues.push(format!("cannot inspect runtime directory: {error}"));
                Vec::new()
            }
        };
        if runtime_files.is_empty() {
            issues.push("runtime directory is empty".to_owned());
        }
        validate_runtime_dll_manifest(&runtime, &runtime_files, &mut issues);
    }

    let revision_path = source.join("SOURCE-REVISION.txt");
    if revision_path.is_file() {
        match fs::read_to_string(&revision_path) {
            Ok(text) => validate_source_revision(&text, source, &mut issues),
            Err(error) => issues.push(format!("cannot read SOURCE-REVISION.txt: {error}")),
        }
    }
    issues
}

fn validate_runtime_dll_manifest(
    runtime: &Path,
    runtime_files: &[fs::DirEntry],
    issues: &mut Vec<String>,
) {
    let manifest = runtime.join("DEPENDENCIES.txt");
    if !manifest.is_file() {
        issues.push(
            "runtime/DEPENDENCIES.txt is missing (exact DLL inventory is required)".to_owned(),
        );
        return;
    }
    let text = match fs::read_to_string(&manifest) {
        Ok(text) => text,
        Err(error) => {
            issues.push(format!("cannot read runtime/DEPENDENCIES.txt: {error}"));
            return;
        }
    };
    let mut declared = BTreeSet::new();
    for line in text.lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('#') {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(".dll") || Path::new(name).file_name().is_none_or(|file| file != name) {
            issues.push(format!(
                "runtime dependency entry is not a DLL basename: {name}"
            ));
            continue;
        }
        if !declared.insert(lower.clone()) {
            issues.push(format!("runtime DLL is declared more than once: {name}"));
        }
        let dependency = runtime.join(name);
        if !dependency.is_file() {
            issues.push(format!("declared runtime DLL is missing: runtime/{name}"));
        } else if fs::metadata(&dependency).is_ok_and(|metadata| metadata.len() == 0) {
            issues.push(format!("declared runtime DLL is empty: runtime/{name}"));
        }
    }
    if declared.is_empty() {
        issues.push("runtime/DEPENDENCIES.txt declares no DLLs".to_owned());
    }

    let actual = runtime_files
        .iter()
        .filter_map(|entry| {
            let path = entry.path();
            let extension = path.extension()?.to_str()?;
            extension
                .eq_ignore_ascii_case("dll")
                .then(|| entry.file_name().to_string_lossy().to_ascii_lowercase())
        })
        .collect::<BTreeSet<_>>();
    for extra in actual.difference(&declared) {
        issues.push(format!(
            "runtime DLL is absent from DEPENDENCIES.txt: {extra}"
        ));
    }
}

fn validate_source_revision(text: &str, source: &Path, issues: &mut Vec<String>) {
    let lower = text.to_ascii_lowercase();
    if lower.contains("not reproducibly associated") {
        issues.push(
            "SOURCE-REVISION.txt explicitly disclaims reproducible executable/source association"
                .to_owned(),
        );
    }
    let source_archive = source.join("source/NASTRAN-95-source.zip");
    let actual_hash = fs::read(&source_archive)
        .ok()
        .map(|bytes| sha256_hex(&bytes));
    // A revision record may also contain a full Git commit hash. Only accept
    // a 64-digit value on the line explicitly labelling the source archive;
    // otherwise a valid commit identifier could be mistaken for the archive
    // digest.
    let declared_hash = text.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        if !(lower.contains("source archive") && lower.contains("sha-256")) {
            return None;
        }
        line.split(|character: char| !character.is_ascii_hexdigit())
            .find(|token| {
                token.len() == 64 && token.chars().all(|character| character.is_ascii_hexdigit())
            })
            .map(str::to_ascii_lowercase)
    });
    match (declared_hash, actual_hash) {
        (Some(declared), Some(actual)) if declared != actual => issues.push(format!(
            "SOURCE-REVISION.txt source-archive hash {declared} does not match SHA-256 {actual}"
        )),
        (Some(_), Some(_)) => {}
        _ => issues.push(
            "SOURCE-REVISION.txt must label the exact 64-digit SHA-256 of source/NASTRAN-95-source.zip"
                .to_owned(),
        ),
    }
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|error| format!("failed to create {}: {error}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read directory entry: {error}"))?;
        let target = destination.join(entry.file_name());
        if entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", entry.path().display()))?
            .is_dir()
        {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("failed to copy {}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn validate_distribution(
    exe: &Path,
    packaged_config: &Path,
    release_manifest: &Path,
    dist_root: &Path,
) -> Result<(), String> {
    // 1. Verify --help output.
    let package_root = exe.parent().unwrap_or(dist_root);
    let output = Command::new(exe)
        .arg("--help")
        .current_dir(package_root)
        .env("ALAS_FROZEN", "1")
        .output()
        .map_err(|e| format!("failed to execute packaged binary: {e}"))?;

    if !output.status.success() {
        return Err("packaged binary --help failed".to_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("Usage: alas") {
        return Err("unexpected --help output from packaged binary".to_owned());
    }
    println!("  [OK] Standalone binary execution (--help)");

    // 2. Verify the manifest and the generated package configuration before
    // invoking the more expensive headless/AVL smoke run.
    validate_release_manifest(release_manifest, package_root)?;
    let packaged_config_metadata = fs::metadata(packaged_config).map_err(|e| {
        format!(
            "packaged configuration template is missing at {}: {e}",
            packaged_config.display()
        )
    })?;
    if packaged_config_metadata.len() == 0 {
        return Err(format!(
            "packaged configuration template is empty: {}",
            packaged_config.display()
        ));
    }
    println!("  [OK] Release manifest and packaged configuration template");

    // 3. Verify a save/read config round-trip and a writable output path that
    // contains spaces. This catches path handling that a repository-local
    // output directory would hide.
    let smoke_root = env::temp_dir().join(format!("ALAS package smoke {}", std::process::id()));
    let _ = fs::remove_dir_all(&smoke_root);
    fs::create_dir_all(&smoke_root)
        .map_err(|e| format!("failed to create portable smoke directory: {e}"))?;
    let roundtrip_config = smoke_root.join("config roundtrip.yaml");
    run_save_config(exe, &roundtrip_config)?;
    let temp_out = smoke_root.join("run output");
    let config_text = path_arg(&roundtrip_config)?;
    let output_text = path_arg(&temp_out)?;
    let status = Command::new(exe)
        .args([
            "--config",
            &config_text,
            "--no-optimize",
            "--no-mission",
            "--plots",
            "--quiet",
            "--output",
            &output_text,
        ])
        .current_dir(package_root)
        .env("ALAS_FROZEN", "1")
        .status()
        .map_err(|e| format!("failed to execute config round-trip smoke run: {e}"))?;

    if !status.success() {
        return Err("packaged binary config round-trip smoke run failed".to_owned());
    }

    let avl_dir = temp_out.join("avl");
    let avl_force_count = fs::read_dir(&avl_dir)
        .map_err(|e| format!("packaged AVL output directory is missing: {e}"))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "ft")
        })
        .count();
    if avl_force_count == 0 {
        return Err("packaged run did not produce AVL total-force files".to_owned());
    }
    let comparison = temp_out.join("plots/model_comparison.svg");
    let comparison_text = fs::read_to_string(&comparison)
        .map_err(|e| format!("packaged Model Comparison figure is missing: {e}"))?;
    if !comparison_text.contains("Athena AVL") {
        return Err("packaged Model Comparison figure does not name Athena AVL".to_owned());
    }

    // The round-trip path above checks a generated config. Run the shipped
    // template as well so a stale or malformed package file cannot hide behind
    // the temporary copy.
    let packaged_out = smoke_root.join("shipped config output");
    let packaged_config_text = path_arg(packaged_config)?;
    let packaged_output_text = path_arg(&packaged_out)?;
    let packaged_status = Command::new(exe)
        .args([
            "--config",
            &packaged_config_text,
            "--no-optimize",
            "--no-mission",
            "--plots",
            "--quiet",
            "--output",
            &packaged_output_text,
        ])
        .current_dir(package_root)
        .env("ALAS_FROZEN", "1")
        .status()
        .map_err(|e| format!("failed to execute shipped configuration smoke run: {e}"))?;
    if !packaged_status.success() {
        return Err("packaged binary failed to read the shipped configuration template".to_owned());
    }
    validate_release_manifest(release_manifest, package_root)?;
    println!(
        "  [OK] Config round-trip, shipped config read, and writable path-with-spaces; bundled AVL completed with {avl_force_count} total-force files and Model Comparison overlay"
    );

    let _ = fs::remove_dir_all(&smoke_root);
    println!("  [OK] Isolated headless execution test passed");
    Ok(())
}

fn validate_release_manifest(path: &Path, package_root: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path)
        .map_err(|e| format!("failed to read release manifest {}: {e}", path.display()))?;
    if !text.trim_start().starts_with('{') || !text.trim_end().ends_with('}') {
        return Err("release manifest is not a JSON object".to_owned());
    }
    let expected_package = package_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "packaged directory name is not valid UTF-8".to_owned())?;
    for required in [
        "\"schema_version\": 1",
        "\"product\": \"ALAS\"",
        "\"version\":",
        "\"package_name\":",
        "\"source_snapshot\":",
        "\"external_tools\":",
        "\"artifacts\":",
    ] {
        if !text.contains(required) {
            return Err(format!(
                "release manifest is missing required field {required}"
            ));
        }
    }
    if !text.contains(&format!(
        "\"package_name\": {}",
        json_string(expected_package)
    )) {
        return Err(format!(
            "release manifest package_name does not match {}",
            package_root.display()
        ));
    }

    let source_manifest = package_root.join(SOURCE_MANIFEST_NAME);
    let source_text = fs::read_to_string(&source_manifest).map_err(|e| {
        format!(
            "source snapshot manifest is missing or unreadable at {}: {e}",
            source_manifest.display()
        )
    })?;
    for required in [
        "\"schema_version\": 1",
        "\"selection\": \"positive-allowlisted-build-closure\"",
        "\"source_root\": \"source/alas\"",
        "\"files\": [",
    ] {
        if !source_text.contains(required) {
            return Err(format!(
                "source snapshot manifest is missing required field {required}"
            ));
        }
    }

    // Recompute every packaged artifact hash and compare it to the recorded
    // entry. This catches a partially copied or modified package before the
    // archive is emitted; the manifest itself is intentionally excluded from
    // this list to avoid a self-referential hash.
    let actual = collect_artifacts(package_root)?;
    verify_manifest_records(parse_artifact_records(&text)?, actual, "release manifest")?;

    let source_root = package_root.join("source").join("alas");
    let source_actual = collect_artifacts(&source_root)?;
    let source_recorded = parse_manifest_records(&source_text, "files")?;
    if !text.contains(&format!("\"file_count\": {}", source_recorded.len())) {
        return Err(
            "release manifest source_snapshot.file_count does not match SOURCE-MANIFEST.json"
                .to_owned(),
        );
    }
    verify_manifest_records(source_recorded, source_actual, "source snapshot manifest")?;
    Ok(())
}

fn parse_artifact_records(text: &str) -> Result<Vec<ArtifactRecord>, String> {
    parse_manifest_records(text, "artifacts")
}

fn parse_manifest_records(text: &str, field: &str) -> Result<Vec<ArtifactRecord>, String> {
    let marker = format!("\"{field}\": [");
    let section = text
        .split_once(&marker)
        .map(|(_, remainder)| remainder)
        .ok_or_else(|| format!("manifest has no {field} array"))?;
    let mut records = Vec::new();
    let mut path = None;
    let mut bytes = None;
    let mut sha256 = None;
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(']') {
            break;
        }
        if let Some(value) = json_field_string(line, "path") {
            path = Some(value);
        }
        if let Some(value) = json_field_usize(line, "bytes") {
            bytes = Some(value);
        }
        if let Some(value) = json_field_string(line, "sha256") {
            sha256 = Some(value);
        }
        if trimmed == "}" || trimmed == "}," {
            let record = ArtifactRecord {
                path: path
                    .take()
                    .ok_or_else(|| "artifact entry has no path".to_owned())?,
                bytes: bytes
                    .take()
                    .ok_or_else(|| "artifact entry has no byte count".to_owned())?,
                sha256: sha256
                    .take()
                    .ok_or_else(|| "artifact entry has no SHA-256".to_owned())?,
            };
            records.push(record);
        }
    }
    if path.is_some() || bytes.is_some() || sha256.is_some() {
        return Err(format!(
            "manifest {field} array contains an incomplete entry"
        ));
    }
    Ok(records)
}

fn verify_manifest_records(
    recorded: Vec<ArtifactRecord>,
    actual: Vec<ArtifactRecord>,
    label: &str,
) -> Result<(), String> {
    let actual_by_path = actual
        .into_iter()
        .map(|artifact| (artifact.path.clone(), artifact))
        .collect::<BTreeMap<_, _>>();
    if recorded.len() != actual_by_path.len() {
        return Err(format!(
            "{label} records {} files but package contains {}",
            recorded.len(),
            actual_by_path.len()
        ));
    }
    let mut seen = BTreeSet::new();
    for artifact in recorded {
        if !seen.insert(artifact.path.clone()) {
            return Err(format!("{label} contains duplicate path {}", artifact.path));
        }
        let Some(actual) = actual_by_path.get(&artifact.path) else {
            return Err(format!("{label} names an absent file: {}", artifact.path));
        };
        if artifact.bytes != actual.bytes || artifact.sha256 != actual.sha256 {
            return Err(format!("{label} hash/size mismatch for {}", artifact.path));
        }
    }
    Ok(())
}

fn json_field_string(line: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\": ");
    let start = line.find(&needle)? + needle.len();
    let value = line[start..].trim_end_matches(',').trim();
    if !(value.starts_with('"') && value.ends_with('"')) {
        return None;
    }
    let inner = &value[1..value.len() - 1];
    Some(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn json_field_usize(line: &str, field: &str) -> Option<usize> {
    let needle = format!("\"{field}\": ");
    let start = line.find(&needle)? + needle.len();
    line[start..].trim_end_matches(',').trim().parse().ok()
}

fn cargo_target_dir(root: &Path) -> PathBuf {
    env::var_os("CARGO_TARGET_DIR").map_or_else(
        || root.join("target"),
        |directory| {
            let path = PathBuf::from(directory);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        },
    )
}

fn create_zip_archive(dist_root: &Path, folder_name: &str, zip_name: &str) -> Result<(), String> {
    if cfg!(windows) {
        let script = format!(
            "Compress-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            powershell_quote(folder_name),
            powershell_quote(zip_name)
        );
        let status = Command::new("powershell")
            .current_dir(dist_root)
            .args(["-NoProfile", "-Command", &script])
            .status()
            .map_err(|e| format!("failed to run powershell Compress-Archive: {e}"))?;

        if !status.success() {
            return Err("Compress-Archive failed".to_owned());
        }
    } else {
        // GNU tar's `-a` selects a compressor from the suffix but does not
        // create ZIP archives. Prefer the Python standard-library zipfile
        // module, then fall back to the ubiquitous `zip` utility.
        let python3 = Command::new("python3")
            .current_dir(dist_root)
            .args(["-m", "zipfile", "-c", zip_name, folder_name])
            .status();
        if python3.is_ok_and(|status| status.success()) {
            return Ok(());
        }
        let python = Command::new("python")
            .current_dir(dist_root)
            .args(["-m", "zipfile", "-c", zip_name, folder_name])
            .status();
        if python.is_ok_and(|status| status.success()) {
            return Ok(());
        }
        let zip = Command::new("zip")
            .current_dir(dist_root)
            .args(["-q", "-r", zip_name, folder_name])
            .status()
            .map_err(|e| format!("failed to run python zipfile or zip packaging: {e}"))?;
        if !zip.success() {
            return Err("python zipfile and zip packaging failed".to_owned());
        }
    }
    Ok(())
}

fn powershell_quote(value: &str) -> String {
    value.replace('\'', "''")
}

/// A small dependency-free SHA-256 implementation keeps xtask's manifest
/// hashes portable without changing the workspace dependency graph.
fn sha256_hex(data: &[u8]) -> String {
    let mut message = data.to_vec();
    let bit_len = (message.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = [
        0x6a09e667_u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    for chunk in message.chunks_exact(64) {
        let mut words = [0_u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let ch = (e & f) ^ ((!e) & g);
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let temp1 = h
                .wrapping_add(sigma1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let temp2 = sigma0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut result = String::with_capacity(64);
    for word in state {
        result.push_str(&format!("{word:08x}"));
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{
        assess_nastran95, bundle_branding, bundle_status_json, package_name,
        parse_artifact_records, sha256_hex, source_path_allowed, target_label, BundleStatus,
        USER_SUPPLIED_EXTERNAL_TOOLS,
    };

    #[test]
    fn distribution_copies_native_branding_artifacts_verbatim() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(PathBuf::from)
            .expect("workspace root beside xtask crate");
        let package =
            std::env::temp_dir().join(format!("alas-branding-package-{}", std::process::id()));
        let _ = fs::remove_dir_all(&package);
        fs::create_dir_all(&package).expect("temporary package directory");

        bundle_branding(&root, &package).expect("native branding files package");
        for name in [
            "app_logo.png",
            "app_text_logo.png",
            "app_branding.png",
            "app_icon.ico",
        ] {
            assert_eq!(
                fs::read(root.join(name)).expect("source branding file"),
                fs::read(package.join(name)).expect("packaged branding file"),
                "packaged branding differs from {name}"
            );
        }

        fs::remove_dir_all(package).expect("temporary package cleanup");
    }

    #[test]
    fn package_name_uses_workspace_version_and_target() {
        assert_eq!(
            package_name(),
            format!("alas-v{}-{}", super::PACKAGE_VERSION, target_label())
        );
        assert!(!package_name().contains("windows-x86_64.zip"));
    }

    #[test]
    fn sha256_matches_nist_short_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn source_allowlist_excludes_external_tools_and_local_data() {
        assert!(source_path_allowed("crates/alas-app/src/main.rs"));
        assert!(source_path_allowed("crates/alas-core/src/tokenizer.rs"));
        assert!(source_path_allowed("crates/alas-aero/data/nn-small.bin"));
        assert!(source_path_allowed(
            "crates/alas-aero/tests/fixtures/vspaero/tr1208.polar"
        ));
        assert!(source_path_allowed(
            "golden/report/reference_render_w39.json"
        ));
        assert!(source_path_allowed(
            "golden/prop_elec/apc_performance_tables.bin"
        ));
        assert!(source_path_allowed(".gitattributes"));
        assert!(source_path_allowed("tools/aircraft_parity.cjs"));
        assert!(source_path_allowed("docs/release-packaging.md"));
        assert!(!source_path_allowed("external tools/AVL-GPL-2.0.txt"));
        assert!(!source_path_allowed(".agent/reports/review.html"));
        assert!(!source_path_allowed("runs/private/output.json"));
        assert!(!source_path_allowed("private/unreviewed.rs"));
        assert!(!source_path_allowed("crates/alas-app/src/private.rs"));
        assert!(!source_path_allowed("crates/alas-app/notes.json"));
        assert!(!source_path_allowed(
            "crates/alas-uav/audits/coupled_verdict.json"
        ));
        assert!(!source_path_allowed("docs/private/manual.pdf"));
        assert!(!source_path_allowed("golden/aero/analysis.json"));
        assert!(!source_path_allowed(
            "golden/prop_elec/Script Python/Data/PER3_12x6E.dat"
        ));
        assert!(!source_path_allowed(
            "assets/textures/natural_earth_110m_land.geojson"
        ));
        assert!(!source_path_allowed("xtask/src/notes.json"));
        assert!(!source_path_allowed("configs/credentials.yaml"));
        assert!(!source_path_allowed("keys/signing.pem"));
    }

    #[test]
    fn runtime_parity_data_outside_the_compile_time_closure_stays_excluded() {
        // The ~85 MB historical Python-reference-derived golden/ tree (read at
        // runtime via `alas_testkit::golden_dir`/`load_json`, not
        // `include_str!`/`include_bytes!`) is a distinct, still-excluded
        // category, per file, pending an individual provenance review of each
        // fixture -- admitting it is a deliberate reproducibility trade
        // documented in docs/release-packaging.md, not a silent gap in this
        // task's compile-time closure.
        assert!(!source_path_allowed("golden/generators/gen_config.py"));
        assert!(!source_path_allowed("golden/mission/mission.json"));
        assert!(!source_path_allowed("golden/report/manifest.json"));
        // A sibling file under the same directory as the one reviewed
        // evidence file must stay excluded: the admission below is an exact
        // single-file allowance, not a `golden/aircraft` prefix rule.
        assert!(!source_path_allowed("golden/aircraft/some_other_file.json"));
    }

    #[test]
    fn the_reviewed_real_aircraft_parity_contract_is_admitted_by_exact_path_only() {
        // Reviewed and admitted (see SOURCE_REVIEWED_EVIDENCE_FILES's doc
        // comment): every numeric anchor is a short factual value with a
        // `cite` key into a `sources` bibliography of title/authority/url
        // only -- no reproduced manufacturer document text. Required by
        // `tools/aircraft_parity.cjs`, itself part of this source snapshot.
        assert!(source_path_allowed(
            "golden/aircraft/real_aircraft_parity.json"
        ));
    }

    #[test]
    fn user_supplied_tools_are_recorded_as_never_bundled_with_a_notices_pointer() {
        let names: Vec<&str> = USER_SUPPLIED_EXTERNAL_TOOLS
            .iter()
            .map(|(name, _)| *name)
            .collect();
        for expected in [
            "mses",
            "vspaero",
            "msc_nastran",
            "msc_patran",
            "flowunsteady",
        ] {
            assert!(
                names.contains(&expected),
                "release manifest tool inventory is missing {expected}"
            );
        }
        for (name, reason) in USER_SUPPLIED_EXTERNAL_TOOLS {
            let json = bundle_status_json(&BundleStatus::user_supplied(reason));
            assert!(
                json.contains("\"status\": \"user_supplied\""),
                "{name} status"
            );
            assert!(
                json.contains("THIRD-PARTY-NOTICES.md"),
                "{name} reason should point to the licence/provenance notices"
            );
        }
    }

    #[test]
    fn missing_nastran_staging_is_reported_without_a_placeholder() {
        let source = std::env::temp_dir().join(format!(
            "alas-nastran-staging-{}-absent",
            std::process::id()
        ));
        let issues = assess_nastran95(&source);
        assert!(issues
            .iter()
            .any(|issue| issue.contains("staging directory is absent")));
        assert!(!source.exists());
    }

    #[test]
    fn artifact_manifest_parser_requires_path_size_and_hash() {
        let text = r#"{
  "artifacts": [
    {
      "path": "alas.exe",
      "bytes": 17,
      "sha256": "0123456789abcdef"
    }
  ]
}"#;
        let records = parse_artifact_records(text).expect("well-formed artifact entry");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, "alas.exe");
        assert_eq!(records[0].bytes, 17);
        assert_eq!(records[0].sha256, "0123456789abcdef");

        let incomplete = text.replace("\"sha256\": \"0123456789abcdef\"", "\"sha256\": 7");
        assert!(parse_artifact_records(&incomplete).is_err());
    }
}
