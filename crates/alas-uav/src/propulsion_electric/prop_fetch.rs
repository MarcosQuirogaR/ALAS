// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Opt-in download of published `PER3` propeller tables to the local data directory.
//!
//! Nothing here runs unless a user starts the `fetch_props` example.  The
//! transport is a seam ([`PropHttpGet`]) so the policy below is tested offline:
//!
//! * robots.txt is fetched first and every URL must be allowed by it;
//! * only `https://www.apcprop.com/files/PER3_*.dat` files are requested,
//!   after reading the maker's public performance-data index page;
//! * at least five seconds separate any two requests;
//! * the User-Agent is generic and carries no personal contact data;
//! * files already present and valid are skipped (resumable), every download
//!   is parsed before it is kept, and three consecutive failures stop the run.

use std::path::Path;
use std::time::Duration;

use super::prop_data::{is_per3_name, is_safe_name, read_directory, store_per3_file};
use crate::collector::{robots_allows, Sleeper};

/// Host the tables are downloaded from.
pub const PROP_SOURCE_HOST: &str = "www.apcprop.com";
/// Public index page that links every table.
pub const PROP_INDEX_URL: &str = "https://www.apcprop.com/technical-information/performance-data/";
/// robots.txt of the host.
pub const PROP_ROBOTS_URL: &str = "https://www.apcprop.com/robots.txt";
/// Generic identity sent with every request (no personal data).
pub const PROP_USER_AGENT: &str =
    "ALAS-PropData/0.1 (noncommercial engineering tool; on-demand user download)";
/// Smallest allowed pause between two requests.
pub const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(5);
const MAX_CONSECUTIVE_FAILURES: usize = 3;

/// Transport used by [`fetch_prop_tables`].
pub trait PropHttpGet {
    /// GET one HTTPS URL and return the body, following redirects within HTTPS.
    fn get(&mut self, url: &str, user_agent: &str) -> Result<Vec<u8>, String>;
}

/// Which published tables to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropSelection {
    /// Exact model names such as `12x6E` (case-insensitive), without `PER3_`/`.dat`.
    Models(Vec<String>),
    /// Every table whose model designation contains an `E` (electric series).
    Electric,
    /// Every published table.
    All,
}

/// Settings of one download run.
#[derive(Debug, Clone)]
pub struct PropFetchOptions {
    /// Tables to fetch.
    pub selection: PropSelection,
    /// Pause between requests; values below [`MIN_REQUEST_INTERVAL`] are raised to it.
    pub interval: Duration,
}

/// What a run did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropFetchReport {
    /// Newly stored files.
    pub downloaded: Vec<String>,
    /// Selected files that were already installed and valid.
    pub already_present: Vec<String>,
    /// Failed files with the reason.
    pub failed: Vec<(String, String)>,
    /// Requested model names the index does not list.
    pub unknown_models: Vec<String>,
}

/// Why a run could not proceed.
#[derive(Debug, thiserror::Error)]
pub enum PropFetchError {
    /// robots.txt forbids a required URL.
    #[error("robots.txt does not allow {0}")]
    RobotsDisallowed(String),
    /// A request failed before any table could be selected.
    #[error("{0}")]
    Transport(String),
    /// The index page lists no table links (page layout changed).
    #[error("the index page lists no PER3 tables; the site layout may have changed")]
    EmptyIndex,
    /// Repeated failures; the partial progress is kept for a later run.
    #[error("stopped after {MAX_CONSECUTIVE_FAILURES} consecutive failures; last: {0}")]
    TooManyFailures(String),
}

/// Download the selected tables into `dest`.  `progress` receives one line per step.
pub fn fetch_prop_tables(
    fetcher: &mut impl PropHttpGet,
    sleeper: &mut impl Sleeper,
    dest: &Path,
    options: &PropFetchOptions,
    progress: &mut dyn FnMut(&str),
) -> Result<PropFetchReport, PropFetchError> {
    let interval = options.interval.max(MIN_REQUEST_INTERVAL);
    let mut first_request = true;
    let mut request = |url: &str, sleeper: &mut dyn FnMut(Duration)| {
        if !std::mem::take(&mut first_request) {
            sleeper(interval);
        }
        fetcher.get(url, PROP_USER_AGENT)
    };
    let mut pause = |duration: Duration| sleeper.sleep(duration);

    let robots = request(PROP_ROBOTS_URL, &mut pause)
        .map_err(|error| PropFetchError::Transport(format!("robots.txt: {error}")))?;
    let robots = String::from_utf8_lossy(&robots).into_owned();
    if !robots_allows(&robots, PROP_INDEX_URL) {
        return Err(PropFetchError::RobotsDisallowed(PROP_INDEX_URL.to_owned()));
    }
    progress("reading the public index page");
    let index = request(PROP_INDEX_URL, &mut pause)
        .map_err(|error| PropFetchError::Transport(format!("index page: {error}")))?;
    let names = index_table_names(&String::from_utf8_lossy(&index));
    if names.is_empty() {
        return Err(PropFetchError::EmptyIndex);
    }

    let (selected, unknown_models) = select_tables(&names, &options.selection);
    let installed = read_directory(Some(dest.to_path_buf()));
    let mut report = PropFetchReport {
        unknown_models,
        ..PropFetchReport::default()
    };
    let mut consecutive_failures = 0;
    for (position, name) in selected.iter().enumerate() {
        if installed.tables.iter().any(|t| t.source_file() == name) {
            report.already_present.push(name.clone());
            continue;
        }
        let url = format!("https://{PROP_SOURCE_HOST}/files/{name}");
        if !robots_allows(&robots, &url) {
            return Err(PropFetchError::RobotsDisallowed(url));
        }
        progress(&format!(
            "downloading {name} ({} of {})",
            position + 1,
            selected.len()
        ));
        let outcome =
            request(&url, &mut pause).and_then(|bytes| store_per3_file(dest, name, &bytes));
        match outcome {
            Ok(()) => {
                consecutive_failures = 0;
                report.downloaded.push(name.clone());
            }
            Err(reason) => {
                consecutive_failures += 1;
                progress(&format!("failed {name}: {reason}"));
                report.failed.push((name.clone(), reason.clone()));
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    return Err(PropFetchError::TooManyFailures(reason));
                }
            }
        }
    }
    super::prop_data::reload_prop_data();
    Ok(report)
}

/// `PER3_*.dat` names linked from the index page, de-duplicated, in page order.
pub(super) fn index_table_names(html: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for piece in html.split("href=\"").skip(1) {
        let Some(link) = piece.split('"').next() else {
            continue;
        };
        let path = link
            .strip_prefix(&format!("https://{PROP_SOURCE_HOST}"))
            .unwrap_or(link);
        let Some(name) = path.strip_prefix("/files/") else {
            continue;
        };
        if is_per3_name(name) && is_safe_name(name) && !names.iter().any(|n| n == name) {
            names.push(name.to_owned());
        }
    }
    names
}

fn model_of(file_name: &str) -> &str {
    let stem = file_name.get("PER3_".len()..).unwrap_or(file_name);
    stem.rsplit_once('.').map_or(stem, |(stem, _)| stem)
}

fn select_tables(names: &[String], selection: &PropSelection) -> (Vec<String>, Vec<String>) {
    match selection {
        PropSelection::All => (names.to_vec(), Vec::new()),
        PropSelection::Electric => (
            names
                .iter()
                .filter(|name| model_of(name).to_ascii_uppercase().contains('E'))
                .cloned()
                .collect(),
            Vec::new(),
        ),
        PropSelection::Models(models) => {
            let mut selected = Vec::new();
            let mut unknown = Vec::new();
            for model in models {
                match names
                    .iter()
                    .find(|name| model_of(name).eq_ignore_ascii_case(model.trim()))
                {
                    Some(name) if !selected.contains(name) => selected.push(name.clone()),
                    Some(_) => {}
                    None => unknown.push(model.clone()),
                }
            }
            (selected, unknown)
        }
    }
}
