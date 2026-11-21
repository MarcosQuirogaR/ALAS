// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tool manifest: what can be installed, from where, with which hash.

use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use super::InstallError;

const BUNDLED: &str = include_str!("../../data/tool-manifest.json");

/// How a tool is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallKind {
    /// ALAS downloads a hash-pinned archive from the publisher.
    Automatic,
    /// Redistributable in principle, but no reviewed archive is pinned yet.
    PendingReview,
    /// Licence-gated or needs administrator rights: steps only.
    Guided,
    /// ALAS downloads a hash-pinned source archive (and, where the platform has
    /// no compiler, a hash-pinned toolchain), patches and compiles it on this
    /// machine and checks the result. Nothing built is ever redistributed.
    BuildFromSource,
}

/// One command of a source build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildStep {
    /// Program, looked up on the toolchain-extended `PATH`.
    pub program: String,
    /// Arguments; `{src}` is the patched source root, `{jobs}` the parallel job count.
    #[serde(default)]
    pub args: Vec<String>,
    /// Wall-clock limit.
    #[serde(default = "default_build_timeout")]
    pub timeout_seconds: u64,
}

fn default_build_timeout() -> u64 {
    3600
}

/// How a `build_from_source` tool is made, by platform key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildSpec {
    /// Name of a patch embedded in the application (see `build::embedded_patch`).
    #[serde(default)]
    pub patch: Option<String>,
    /// Hash-pinned toolchain archives per platform, extracted once below
    /// `<user data>/toolchains/<directory>` and reused.
    #[serde(default)]
    pub toolchain: BTreeMap<String, Vec<Artifact>>,
    /// Directories (relative to each toolchain's directory) put first on `PATH`.
    #[serde(default)]
    pub path_dirs: BTreeMap<String, Vec<String>>,
    /// Programs the system must already provide on a platform with no toolchain archive.
    #[serde(default)]
    pub required_commands: BTreeMap<String, Vec<String>>,
    /// Disk the build tree needs while compiling, shown before consent.
    #[serde(default)]
    pub disk_bytes: Option<u64>,
    /// What to tell the user when `required_commands` are missing.
    #[serde(default)]
    pub missing_commands_hint: BTreeMap<String, String>,
    /// The commands, in order.
    #[serde(default)]
    pub steps: BTreeMap<String, Vec<BuildStep>>,
    /// Paths (relative to the source root) kept in the installed directory; the
    /// rest of the tree is removed after a successful build and check.
    #[serde(default)]
    pub keep: Vec<String>,
    /// Name of the self-check the application registered for this tool.
    #[serde(default)]
    pub check: Option<String>,
}

/// Archive container understood by the platform `tar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchiveFormat {
    /// ZIP (read by Windows `tar.exe`, which is bsdtar).
    #[serde(rename = "zip")]
    Zip,
    /// Uncompressed tar.
    #[serde(rename = "tar")]
    Tar,
    /// Gzip-compressed tar.
    #[serde(rename = "tar.gz")]
    TarGz,
    /// XZ-compressed tar.
    #[serde(rename = "tar.xz")]
    TarXz,
}

/// A tiny case run against the installed executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthSpec {
    /// Arguments; `{dir}` expands to the scratch directory holding `case_files`.
    #[serde(default)]
    pub args: Vec<String>,
    /// Files written to the scratch directory before the run (name to text).
    #[serde(default)]
    pub case_files: BTreeMap<String, String>,
    /// Text that must appear on stdout or stderr.
    #[serde(default)]
    pub expect_stdout_contains: Option<String>,
    /// Wall-clock limit.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_timeout() -> u64 {
    60
}

/// One downloadable archive for one platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// Publisher version string.
    pub version: String,
    /// Licence of this archive, when it differs from the tool's (a toolchain).
    #[serde(default)]
    pub licence: Option<String>,
    /// HTTPS source.
    pub url: String,
    /// Pinned SHA-256 (lowercase hex). `None` blocks installation.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Expected size in bytes, for progress display.
    #[serde(default)]
    pub size_bytes: Option<u64>,
    /// Smallest size accepted as a complete transfer.
    pub min_bytes: u64,
    /// Disk the unpacked archive takes, when it is much larger than the archive.
    #[serde(default)]
    pub extracted_bytes: Option<u64>,
    /// Container format.
    pub format: ArchiveFormat,
    /// Directory name below `external tools`.
    pub directory: String,
    /// Use the archive's single top-level directory as the root.
    #[serde(default)]
    pub strip_single_root: bool,
    /// Executable inside the directory that the application should use.
    pub executable: String,
    /// Console executable for the health check when `executable` is a GUI.
    #[serde(default)]
    pub health_executable: Option<String>,
    /// Tiny case that proves the install works.
    #[serde(default)]
    pub health: Option<HealthSpec>,
}

/// One external tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolEntry {
    /// Stable identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Licence summary shown on the consent screen.
    pub licence: String,
    /// Publisher page.
    pub publisher_url: String,
    /// How the tool is obtained.
    pub kind: InstallKind,
    /// One-line description.
    #[serde(default)]
    pub summary: String,
    /// Steps for guided tools.
    #[serde(default)]
    pub guided_steps: Vec<String>,
    /// Artifacts by platform key (`windows-x86_64`, `linux-x86_64`).
    #[serde(default)]
    pub artifacts: BTreeMap<String, Artifact>,
    /// How a `build_from_source` tool is built.
    #[serde(default)]
    pub build: Option<BuildSpec>,
    /// Application setting that should point at the executable, if any.
    #[serde(default)]
    pub registers: Option<String>,
}

/// The full tool list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Schema version (currently 1).
    pub schema: u32,
    /// Tools in display order.
    pub tools: Vec<ToolEntry>,
}

impl Manifest {
    /// The manifest compiled into the application.
    pub fn bundled() -> Result<Self, InstallError> {
        Self::parse(BUNDLED)
    }

    /// Parse and validate a manifest.
    pub fn parse(text: &str) -> Result<Self, InstallError> {
        let manifest: Manifest = serde_json::from_str(text)
            .map_err(|error| InstallError::Manifest(error.to_string()))?;
        if manifest.schema != 1 {
            return Err(InstallError::Manifest(format!(
                "unsupported schema {}",
                manifest.schema
            )));
        }
        for tool in &manifest.tools {
            for (platform, artifact) in &tool.artifacts {
                validate_artifact(&tool.id, platform, artifact)?;
            }
            if let Some(build) = &tool.build {
                for (platform, parts) in &build.toolchain {
                    for part in parts {
                        validate_artifact(&tool.id, &format!("{platform} toolchain"), part)?;
                    }
                }
            }
            if tool.kind == InstallKind::BuildFromSource && tool.build.is_none() {
                return Err(InstallError::Manifest(format!(
                    "{}: build_from_source needs a build section",
                    tool.id
                )));
            }
        }
        Ok(manifest)
    }

    /// Look up a tool by id.
    pub fn tool(&self, id: &str) -> Option<&ToolEntry> {
        self.tools.iter().find(|tool| tool.id == id)
    }
}

fn validate_artifact(id: &str, platform: &str, artifact: &Artifact) -> Result<(), InstallError> {
    let fail = |what: &str| Err(InstallError::Manifest(format!("{id}/{platform}: {what}")));
    if let Some(hash) = &artifact.sha256 {
        let valid = hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if !valid {
            return fail("sha256 must be 64 lowercase hex digits");
        }
    }
    let directory = Path::new(&artifact.directory);
    let plain = directory.components().count() == 1
        && matches!(directory.components().next(), Some(Component::Normal(_)));
    if !plain {
        return fail("directory must be a single plain name");
    }
    let executable = Path::new(&artifact.executable);
    let escapes = executable.is_absolute()
        || executable
            .components()
            .any(|part| !matches!(part, Component::Normal(_)));
    if escapes {
        return fail("executable must be a relative path without '..'");
    }
    Ok(())
}

impl ToolEntry {
    /// The artifact for this platform, if the manifest has one.
    pub fn artifact_for(&self, platform: &str) -> Option<&Artifact> {
        self.artifacts.get(platform)
    }
}

/// Platform key of the running process.
pub fn current_platform() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "windows-x86_64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x86_64"
    } else {
        "unsupported"
    }
}
