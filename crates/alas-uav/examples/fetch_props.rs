// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Install propeller performance tables on this machine (opt-in, on demand).
//!
//! ALAS does not distribute APC propeller performance data.  This tool either
//! downloads the published `PER3_*.dat` files politely (robots.txt respected,
//! at least 5 s between requests, generic User-Agent, resumable) using the
//! system `curl`, or imports files you already have.
//!
//! ```text
//! cargo run -p alas-uav --release --example fetch_props -- --status
//! cargo run -p alas-uav --release --example fetch_props -- --models 12x6E,10x5E
//! cargo run -p alas-uav --release --example fetch_props -- --electric
//! cargo run -p alas-uav --release --example fetch_props -- --import C:\path\to\folder-or.zip
//! ```
//!
//! Options: `--dir PATH` overrides the data directory (default: see
//! `alas_uav::propulsion_electric::prop_data`); `--all` fetches every table.
//! The files remain APC's property; check APC's terms before redistributing.

// The example owns the console; library code reports through typed values.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use alas_uav::collector::SystemSleeper;
use alas_uav::propulsion_electric::prop_data::{
    import_prop_data, prop_data_dir, prop_data_status, set_prop_data_dir,
};
use alas_uav::propulsion_electric::prop_fetch::{
    fetch_prop_tables, PropFetchOptions, PropHttpGet, PropSelection, MIN_REQUEST_INTERVAL,
};

struct CurlGet;

impl PropHttpGet for CurlGet {
    fn get(&mut self, url: &str, user_agent: &str) -> Result<Vec<u8>, String> {
        let output = Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--max-redirs",
                "3",
                "--max-time",
                "60",
                "--max-filesize",
                "8000000",
                "--user-agent",
                user_agent,
                url,
            ])
            .output()
            .map_err(|error| format!("curl could not start: {error}"))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
        }
    }
}

fn main() {
    if let Err(message) = run() {
        eprintln!("error: {message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = std::env::args().skip(1);
    let mut selection: Option<PropSelection> = None;
    let mut import: Option<PathBuf> = None;
    let mut status_only = false;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--dir" => {
                let value = arguments.next().ok_or("--dir needs a path")?;
                set_prop_data_dir(Some(PathBuf::from(value)));
            }
            "--models" => {
                let value = arguments.next().ok_or("--models needs a comma list")?;
                selection = Some(PropSelection::Models(
                    value.split(',').map(|m| m.trim().to_owned()).collect(),
                ));
            }
            "--electric" => selection = Some(PropSelection::Electric),
            "--all" => selection = Some(PropSelection::All),
            "--import" => {
                import = Some(PathBuf::from(
                    arguments.next().ok_or("--import needs a folder or .zip")?,
                ));
            }
            "--status" => status_only = true,
            other => return Err(format!("unknown argument {other}; see the file header")),
        }
    }
    let dest = prop_data_dir().ok_or("no data directory; pass --dir or set ALAS_PROP_DATA_DIR")?;

    if let Some(source) = import {
        let report = import_prop_data(&source, &dest).map_err(|error| error.to_string())?;
        println!(
            "imported {} file(s) into {}",
            report.imported.len(),
            dest.display()
        );
        for (name, reason) in &report.rejected {
            println!("rejected {name}: {reason}");
        }
    } else if let Some(selection) = selection {
        println!(
            "Fetching propeller tables into {}\n\
             Requests are spaced by at least {} s; this can take a while. Ctrl+C is safe: \
             completed files are kept and skipped next time.",
            dest.display(),
            MIN_REQUEST_INTERVAL.as_secs()
        );
        let options = PropFetchOptions {
            selection,
            interval: Duration::from_secs(5),
        };
        let report = fetch_prop_tables(
            &mut CurlGet,
            &mut SystemSleeper,
            &dest,
            &options,
            &mut |line| println!("{line}"),
        )
        .map_err(|error| error.to_string())?;
        println!(
            "downloaded {}, already present {}, failed {}",
            report.downloaded.len(),
            report.already_present.len(),
            report.failed.len()
        );
        for model in &report.unknown_models {
            println!("not listed by the publisher: {model}");
        }
    } else if !status_only {
        return Err("choose --models LIST, --electric, --all, --import PATH, or --status".into());
    }

    alas_uav::propulsion_electric::prop_data::reload_prop_data();
    let status = prop_data_status();
    println!(
        "{} propeller table(s) installed in {}",
        status.table_count,
        dest.display()
    );
    for (name, reason) in &status.skipped {
        println!("unusable file {name}: {reason}");
    }
    Ok(())
}
