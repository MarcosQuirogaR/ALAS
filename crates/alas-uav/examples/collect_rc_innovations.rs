// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Optional live RC Innovations collector using the system curl executable.

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

use alas_uav::collector::{collect, CrawlPolicy, FetchRequest, PageFetcher, SystemSleeper};

struct CurlFetcher;

impl PageFetcher for CurlFetcher {
    fn fetch(&mut self, request: FetchRequest<'_>) -> Result<String, String> {
        let output = Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--proto",
                "=https",
                "--max-time",
                "30",
                "--user-agent",
                request.user_agent,
                request.url,
            ])
            .output()
            .map_err(|error| format!("curl could not start: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
        }
        String::from_utf8(output.stdout).map_err(|error| format!("response is not UTF-8: {error}"))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    if arguments.next().as_deref() != Some("--live") {
        return Err("live network access requires an explicit --live argument".into());
    }
    let output_path = PathBuf::from(
        arguments
            .next()
            .unwrap_or_else(|| "rc_innovations_collected.json".to_owned()),
    );
    let mut policy = CrawlPolicy::default();
    if let Some(page_cap) = arguments.next() {
        policy.max_product_pages = page_cap.parse()?;
        policy.max_category_pages = policy.max_product_pages.max(1);
    }
    let mut fetcher = CurlFetcher;
    let mut sleeper = SystemSleeper;
    let collected = collect(&mut fetcher, &mut sleeper, &policy)?;
    let json = serde_json::to_string_pretty(&collected)?;
    std::fs::write(&output_path, json + "\n")?;
    Ok(())
}
