// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Packaging and distribution assembly task for xtask.
//!
//! Creates standalone release distributions containing the single-binary executable,
//! the unchanged GPL AVL child executable, configuration templates, licences,
//! notices, and documentation.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Build and package the release distribution archive.
pub fn create_distribution(root: &Path) -> Result<(), String> {
    println!("Creating standalone release distribution for ALAS...\n");

    // 1. Compile optimized release binary
    println!("Compiling release binary...");
    let status = Command::new(env!("CARGO"))
        .current_dir(root)
        .args(["build", "--release", "--bin", "alas"])
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

    // 2. Assemble distribution directory structure
    let package_name = "alas-v0.1.0-windows-x86_64";
    let dist_root = root.join("dist");
    let pkg_dir = dist_root.join(package_name);

    if pkg_dir.exists() {
        fs::remove_dir_all(&pkg_dir)
            .map_err(|e| format!("failed to clean {}: {e}", pkg_dir.display()))?;
    }
    fs::create_dir_all(&pkg_dir)
        .map_err(|e| format!("failed to create {}: {e}", pkg_dir.display()))?;

    // Copy executable
    let dst_exe = pkg_dir.join(exe_name);
    fs::copy(&src_exe, &dst_exe)
        .map_err(|e| format!("failed to copy binary to {}: {e}", dst_exe.display()))?;
    println!("Copied standalone binary: {}", dst_exe.display());

    bundle_avl(root, &pkg_dir)?;
    bundle_nastran95(root, &pkg_dir)?;

    // Copy documentation and notices
    for doc in ["README.md", "LICENSE", "NOTICE", "THIRD-PARTY-NOTICES.md"] {
        let src_doc = root.join(doc);
        if src_doc.exists() {
            let dst_doc = pkg_dir.join(doc);
            fs::copy(&src_doc, &dst_doc)
                .map_err(|e| format!("failed to copy {doc} to {}: {e}", dst_doc.display()))?;
        }
    }

    // 3. Export configuration templates
    let configs_dir = pkg_dir.join("configs");
    fs::create_dir_all(&configs_dir)
        .map_err(|e| format!("failed to create {}: {e}", configs_dir.display()))?;

    let sample_config = configs_dir.join("ave.yaml");
    let status = Command::new(&dst_exe)
        .args([
            "--save-config",
            sample_config.to_str().unwrap_or("ave.yaml"),
        ])
        .status()
        .map_err(|e| format!("failed to export sample configuration: {e}"))?;

    if !status.success() {
        return Err("failed to generate sample configuration file".to_owned());
    }
    println!(
        "Generated configuration templates in {}",
        configs_dir.display()
    );

    // 4. Create distribution zip archive
    let zip_name = format!("{package_name}.zip");
    let zip_path = dist_root.join(&zip_name);
    if zip_path.exists() {
        let _ = fs::remove_file(&zip_path);
    }

    println!("Creating package archive: {}", zip_path.display());
    create_zip_archive(&dist_root, package_name, &zip_name)?;

    // 5. Validate the packaged standalone distribution
    println!("\nValidating packaged standalone distribution...");
    validate_distribution(&dst_exe, &dist_root)?;

    println!("\nDistribution packaging complete: {}", zip_path.display());
    Ok(())
}

fn cargo_target_dir(root: &Path) -> std::path::PathBuf {
    env::var_os("CARGO_TARGET_DIR").map_or_else(
        || root.join("target"),
        |directory| {
            let path = std::path::PathBuf::from(directory);
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
        let script =
            format!("Compress-Archive -Path '{folder_name}' -DestinationPath '{zip_name}' -Force");
        let status = Command::new("powershell")
            .current_dir(dist_root)
            .args(["-NoProfile", "-Command", &script])
            .status()
            .map_err(|e| format!("failed to run powershell Compress-Archive: {e}"))?;

        if !status.success() {
            return Err("Compress-Archive failed".to_owned());
        }
    } else {
        let status = Command::new("tar")
            .current_dir(dist_root)
            .args(["-a", "-c", "-f", zip_name, folder_name])
            .status()
            .map_err(|e| format!("failed to run tar: {e}"))?;

        if !status.success() {
            return Err("tar packaging failed".to_owned());
        }
    }
    Ok(())
}

fn validate_distribution(exe: &Path, dist_root: &Path) -> Result<(), String> {
    // 1. Verify --help output
    let output = Command::new(exe)
        .arg("--help")
        .current_dir(exe.parent().unwrap_or(dist_root))
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

    // 2. Verify dry-run headless execution in isolated temporary directory
    let package_root = exe.parent().unwrap_or(dist_root);
    let temp_out = package_root.join(format!("test_package_output-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_out);

    let status = Command::new(exe)
        .args([
            "--no-optimize",
            "--no-mission",
            "--plots",
            "--quiet",
            "--output",
            temp_out.to_str().unwrap_or("test_package_output"),
        ])
        .current_dir(exe.parent().unwrap_or(dist_root))
        .env("ALAS_FROZEN", "1")
        .status()
        .map_err(|e| format!("failed to execute test run on packaged binary: {e}"))?;

    if !status.success() {
        return Err("packaged binary test run failed".to_owned());
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
    println!(
        "  [OK] Bundled AVL completed with {avl_force_count} total-force files and Model Comparison overlay"
    );

    if temp_out.exists() {
        let _ = fs::remove_dir_all(&temp_out);
    }
    println!("  [OK] Isolated headless execution test passed");

    Ok(())
}

/// Copy the unchanged GPL AVL program and its corresponding source into the
/// distribution as an adjacent child executable.
///
/// Keeping this process boundary is what lets the AGPL application and the
/// GPL solver remain separately licensed works. The source archive and the
/// full GPL text travel with the exact executable used by the package.
fn bundle_avl(root: &Path, package_dir: &Path) -> Result<(), String> {
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
    Ok(())
}

/// Copy NASTRAN-95 as a separately licensed adjacent program.
///
/// The staging directory is deliberately strict: packaging is refused unless
/// the exact executable, rigid formats, runtime, corresponding source, NOSA
/// text, and local change record have all been assembled and reviewed.
fn bundle_nastran95(root: &Path, package_dir: &Path) -> Result<(), String> {
    let source = root.join("external tools").join("NASTRAN-95");
    let required_files = [
        "build/bin/nastran.exe",
        "build/bin/nastran95-build.txt",
        "rf/NASINFO",
        "LICENSE",
        "MODIFICATIONS.md",
        "SOURCE-REVISION.txt",
        "source/NASTRAN-95-source.zip",
    ];
    for relative in required_files {
        let artifact = source.join(relative);
        if !artifact.is_file() {
            return Err(format!(
                "NASTRAN-95 bundle compliance artifact is missing: {}; release packaging is intentionally blocked",
                artifact.display()
            ));
        }
    }
    let runtime = source.join("runtime");
    if !runtime.is_dir()
        || fs::read_dir(&runtime)
            .map_err(|error| format!("failed to inspect {}: {error}", runtime.display()))?
            .next()
            .is_none()
    {
        return Err(format!(
            "NASTRAN-95 runtime directory is missing or empty: {}",
            runtime.display()
        ));
    }

    let destination = package_dir.join("external tools").join("NASTRAN-95");
    copy_directory(&source, &destination)?;
    println!(
        "Bundled separately licensed NASTRAN-95 executable, runtime, rigid formats, corresponding source, and notices in {}",
        destination.display()
    );
    Ok(())
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
