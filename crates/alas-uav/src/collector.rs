// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evidence-preserving collection of RC component retail pages.
//!
//! Crawling keeps evidence and volatile procurement text outside reviewed physics.

use std::collections::{BTreeSet, VecDeque};
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[path = "collector_sources.rs"]
mod sources;
pub use sources::{built_in_sources, collect_sources, SourceAdapter};
#[path = "collector_policy.rs"]
mod policy;
pub use policy::CrawlPolicy;

/// Default identity sent by the optional live collector.
pub const DEFAULT_USER_AGENT: &str =
    "ALAS-UAV-Catalog-Research/0.1 (noncommercial engineering catalogue)";

/// One transport request after policy validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest<'a> {
    /// Allowed HTTPS URL.
    pub url: &'a str,
    /// User-agent required by the crawl policy.
    pub user_agent: &'a str,
}

/// Transport seam used by live and deterministic offline collectors.
pub trait PageFetcher {
    /// Fetch one UTF-8 response body without following the request elsewhere.
    fn fetch(&mut self, request: FetchRequest<'_>) -> Result<String, String>;
}

/// Delay seam that makes rate limiting testable without wall-clock sleeps.
pub trait Sleeper {
    /// Wait before the next request.
    fn sleep(&mut self, duration: Duration);
}

/// Wall-clock sleeper used for live collection.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemSleeper;

impl Sleeper for SystemSleeper {
    fn sleep(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Broad category assigned before physical normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentCategory {
    /// Propulsion battery.
    Battery,
    /// Electric motor.
    Motor,
    /// ESC, BEC, SBEC, or UBEC.
    EscBec,
    /// Fixed-wing propeller.
    Propeller,
    /// Servo actuator.
    Servo,
    /// Radio receiver or telemetry sensor.
    ReceiverTelemetry,
    /// Airframe material stock.
    Material,
    /// Landing-gear hardware.
    LandingGear,
    /// Other potentially relevant onboard electronics.
    Electronics,
    /// Page retained for review but not classified.
    Other,
}

/// Review state of a collected page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    /// Collected evidence has not been reviewed into a physical record.
    NeedsReview,
    /// A reviewed record exists in the normalized catalogue under this page.
    Normalized,
    /// The page is relevant but publishes insufficient physical information.
    EvidenceGap,
}

/// Commercial facts retained outside the physics schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementMetadata {
    /// Price text exactly as exposed by the visible page.
    pub price_text: Option<String>,
    /// Availability text exactly as exposed by the visible page.
    pub availability_text: Option<String>,
    /// These values are volatile and must never be treated as design ratings.
    pub volatile: bool,
}

/// One collected product page before normalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectedProduct {
    /// Stable product page URL without query parameters or fragments.
    pub source_url: String,
    /// Page heading used during review.
    pub source_title: String,
    /// Category inferred from URL and visible title.
    pub category: ComponentCategory,
    /// Short visible lines carrying possible physical evidence.
    pub raw_evidence: Vec<String>,
    /// Current commercial state, isolated from physics.
    pub procurement: ProcurementMetadata,
    /// Review disposition.
    pub review_status: ReviewStatus,
    /// Stable normalized id after review, otherwise absent.
    pub normalized_component_id: Option<String>,
}

/// Reproducible output of a bounded crawl.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectedCatalogue {
    /// Collector artifact schema revision.
    pub schema_version: u32,
    /// Stable source-adapter identifier.
    pub source_id: String,
    /// Publisher whose pages were collected.
    pub publisher: String,
    /// UTC-compatible Unix timestamp captured before the first request.
    pub collection_started_unix_s: u64,
    /// Robots policy checked before the crawl.
    pub robots_url: String,
    /// Sitemap actually read.
    pub sitemap_url: String,
    /// User-agent actually sent.
    pub user_agent: String,
    /// Policy delay in milliseconds.
    pub min_request_interval_ms: u64,
    /// Number of relevant URLs discovered before the page cap.
    pub discovered_product_urls: usize,
    /// Relevant product URLs found directly in the sitemap.
    pub sitemap_product_urls: usize,
    /// Additional product URLs found by category and pagination discovery.
    pub category_product_urls: usize,
    /// Category pages fetched under the category page cap.
    pub category_pages_fetched: usize,
    /// Collected pages in deterministic sitemap order.
    pub products: Vec<CollectedProduct>,
}

/// A collection, policy, or parsing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorError {
    message: String,
}

impl CollectorError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CollectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CollectorError {}

/// Collect relevant product pages under a validated crawl policy.
pub fn collect(
    fetcher: &mut impl PageFetcher,
    sleeper: &mut impl Sleeper,
    policy: &CrawlPolicy,
) -> Result<CollectedCatalogue, CollectorError> {
    policy.validate()?;
    let collection_started_unix_s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| CollectorError::new(format!("system clock precedes Unix epoch: {error}")))?
        .as_secs();
    let robots = fetch(fetcher, &policy.robots_url, policy)?;
    let category_scope_allowed = policy
        .category_path_fragments
        .first()
        .map(|fragment| {
            let origin = format!("https://{}{}", policy.allowed_hosts[0], fragment);
            robots_allows(&robots, &origin)
        })
        .unwrap_or(true);
    if !robots_allows(&robots, &policy.sitemap_url) || !category_scope_allowed {
        return Err(CollectorError::new(
            "robots policy does not allow the sitemap and shop paths",
        ));
    }
    let delay = Duration::from_millis(policy.min_request_interval_ms);
    sleeper.sleep(delay);
    let sitemap = fetch(fetcher, &policy.sitemap_url, policy)?;
    let mut urls = discover_product_urls_with_policy(&sitemap, policy)?;
    let sitemap_product_urls = urls.len();
    let mut seen_products = urls.iter().cloned().collect::<BTreeSet<_>>();
    let mut category_queue = policy
        .seed_category_urls
        .iter()
        .cloned()
        .collect::<VecDeque<_>>();
    let mut seen_categories = BTreeSet::new();
    let mut category_pages_fetched = 0;
    while let Some(category_url) = category_queue.pop_front() {
        if category_pages_fetched >= policy.max_category_pages
            || !seen_categories.insert(category_url.clone())
        {
            continue;
        }
        sleeper.sleep(delay);
        let html = fetch(fetcher, &category_url, policy)?;
        category_pages_fetched += 1;
        let links = discover_page_links_with_policy(&category_url, &html, policy)?;
        for link in links {
            if is_product_url_with_policy(&link, policy) && seen_products.insert(link.clone()) {
                urls.push(link);
            } else if is_category_pagination(&category_url, &link, policy)
                && !seen_categories.contains(&link)
            {
                category_queue.push_back(link);
            }
        }
    }
    let category_product_urls = urls.len() - sitemap_product_urls;
    let discovered_product_urls = urls.len();
    let mut products = Vec::with_capacity(urls.len().min(policy.max_product_pages));
    for url in urls.into_iter().take(policy.max_product_pages) {
        sleeper.sleep(delay);
        let html = fetch(fetcher, &url, policy)?;
        products.push(parse_product_page(&url, &html)?);
    }

    Ok(CollectedCatalogue {
        schema_version: 1,
        source_id: policy.source_id.clone(),
        publisher: policy.publisher.clone(),
        collection_started_unix_s,
        robots_url: policy.robots_url.clone(),
        sitemap_url: policy.sitemap_url.clone(),
        user_agent: policy.user_agent.clone(),
        min_request_interval_ms: policy.min_request_interval_ms,
        discovered_product_urls,
        sitemap_product_urls,
        category_product_urls,
        category_pages_fetched,
        products,
    })
}

/// Extract absolute, allow-listed shop links from one saved category page.
pub fn discover_page_links(
    page_url: &str,
    html: &str,
    policy: &CrawlPolicy,
) -> Result<Vec<String>, CollectorError> {
    discover_page_links_with_policy(page_url, html, policy)
}

fn discover_page_links_with_policy(
    page_url: &str,
    html: &str,
    policy: &CrawlPolicy,
) -> Result<Vec<String>, CollectorError> {
    ensure_allowed_url(page_url, policy)?;
    let host = https_host(page_url)
        .ok_or_else(|| CollectorError::new("category page URL has no HTTPS host"))?;
    let mut links = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut offset = 0;
    while let Some(relative_start) = lower[offset..].find("href=") {
        let start = offset + relative_start + 5;
        let Some(quote) = html[start..].chars().next() else {
            break;
        };
        if quote != '\'' && quote != '"' {
            offset = start;
            continue;
        }
        let value_start = start + quote.len_utf8();
        let Some(relative_end) = html[value_start..].find(quote) else {
            break;
        };
        let value = decode_entities(&html[value_start..value_start + relative_end]);
        offset = value_start + relative_end + quote.len_utf8();
        let absolute = if value.starts_with("https://") {
            value
        } else if value.starts_with('/') {
            format!("https://{host}{value}")
        } else {
            continue;
        };
        let canonical = canonical_url(&absolute);
        if ensure_allowed_url(&canonical, policy).is_ok()
            && (is_product_url_with_policy(&canonical, policy)
                || is_category_url(&canonical, policy))
            && !links.contains(&canonical)
        {
            links.push(canonical);
        }
    }
    Ok(links)
}

/// Evaluate the longest matching `Allow` or `Disallow` rule for `User-agent: *`.
pub fn robots_allows(robots_txt: &str, url: &str) -> bool {
    let path = match https_host(url) {
        Some(host) => {
            let rest = &url["https://".len() + host.len()..];
            if rest.is_empty() {
                "/"
            } else {
                rest
            }
        }
        None => return false,
    };
    let mut active = false;
    let mut best: Option<(usize, bool)> = None;
    for raw_line in robots_txt.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "user-agent" {
            active = value == "*";
            continue;
        }
        if !active || (name != "allow" && name != "disallow") || value.is_empty() {
            continue;
        }
        if path.starts_with(value) {
            let candidate = (value.len(), name == "allow");
            if best.is_none_or(|current| {
                candidate.0 > current.0 || (candidate.0 == current.0 && candidate.1 && !current.1)
            }) {
                best = Some(candidate);
            }
        }
    }
    best.map(|(_, allowed)| allowed).unwrap_or(true)
}

/// Discover fixed-wing-relevant product URLs from one sitemap document.
pub fn discover_product_urls(
    sitemap_xml: &str,
    policy: &CrawlPolicy,
) -> Result<Vec<String>, CollectorError> {
    discover_product_urls_with_policy(sitemap_xml, policy)
}

fn discover_product_urls_with_policy(
    sitemap_xml: &str,
    policy: &CrawlPolicy,
) -> Result<Vec<String>, CollectorError> {
    let mut urls = Vec::new();
    let mut remaining = sitemap_xml;
    while let Some(start) = remaining.find("<loc>") {
        remaining = &remaining[start + 5..];
        let Some(end) = remaining.find("</loc>") else {
            return Err(CollectorError::new(
                "sitemap contains an unterminated loc element",
            ));
        };
        let decoded = decode_entities(remaining[..end].trim());
        remaining = &remaining[end + 6..];
        let canonical = canonical_url(&decoded);
        if !is_relevant_product_url_with_policy(&canonical, policy) {
            continue;
        }
        ensure_allowed_url(&canonical, policy)?;
        if !urls.contains(&canonical) {
            urls.push(canonical);
        }
    }
    Ok(urls)
}

/// Parse one saved product page without performing network access.
pub fn parse_product_page(url: &str, html: &str) -> Result<CollectedProduct, CollectorError> {
    if !url.starts_with("https://") {
        return Err(CollectorError::new("product page provenance is not HTTPS"));
    }
    let lines = visible_lines(html);
    let source_title = extract_element_text(html, "h1")
        .or_else(|| extract_element_text(html, "title"))
        .filter(|title| !title.trim().is_empty())
        .ok_or_else(|| CollectorError::new(format!("product page '{url}' has no title")))?;
    let category = classify(&(url.to_owned() + " " + &source_title));
    let raw_evidence = evidence_lines(&lines, &source_title);
    let procurement = ProcurementMetadata {
        price_text: lines.iter().find(|line| is_price_line(line)).cloned(),
        availability_text: lines
            .iter()
            .find(|line| is_availability_line(line))
            .cloned(),
        volatile: true,
    };
    Ok(CollectedProduct {
        source_url: canonical_url(url),
        source_title,
        category,
        raw_evidence,
        procurement,
        review_status: ReviewStatus::NeedsReview,
        normalized_component_id: None,
    })
}

fn fetch(
    fetcher: &mut impl PageFetcher,
    url: &str,
    policy: &CrawlPolicy,
) -> Result<String, CollectorError> {
    ensure_allowed_url(url, policy)?;
    fetcher
        .fetch(FetchRequest {
            url,
            user_agent: &policy.user_agent,
        })
        .map_err(|error| CollectorError::new(format!("could not fetch '{url}': {error}")))
}

fn ensure_allowed_url(url: &str, policy: &CrawlPolicy) -> Result<(), CollectorError> {
    let host = https_host(url).ok_or_else(|| {
        CollectorError::new(format!("crawl URL is not a canonical HTTPS URL: '{url}'"))
    })?;
    if policy.allowed_hosts.iter().any(|allowed| allowed == host) {
        Ok(())
    } else {
        Err(CollectorError::new(format!(
            "crawl URL host '{host}' is outside the allow-list"
        )))
    }
}

fn https_host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    (!host.is_empty() && !host.contains('@') && !host.contains(':')).then_some(host)
}

fn canonical_url(url: &str) -> String {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    url[..end].trim_end_matches('/').to_owned()
}

fn is_relevant_product_url_with_policy(url: &str, policy: &CrawlPolicy) -> bool {
    let lower = url.to_ascii_lowercase();
    let path = lower
        .split_once("://")
        .and_then(|(_, rest)| rest.find('/').map(|index| &rest[index..]))
        .unwrap_or(&lower);
    is_product_url_with_policy(&lower, policy)
        && policy
            .relevance_terms
            .iter()
            .any(|needle| path.contains(needle))
}

fn is_product_url_with_policy(url: &str, policy: &CrawlPolicy) -> bool {
    policy.product_path_fragments.iter().any(|fragment| {
        url.contains(fragment)
            && !policy
                .category_path_fragments
                .iter()
                .any(|category| url.contains(category))
    })
}

fn is_category_url(url: &str, policy: &CrawlPolicy) -> bool {
    policy
        .category_path_fragments
        .iter()
        .any(|fragment| url.contains(fragment))
}

fn is_category_pagination(seed_or_page: &str, candidate: &str, policy: &CrawlPolicy) -> bool {
    let base = seed_or_page
        .split("/page/")
        .next()
        .unwrap_or(seed_or_page)
        .trim_end_matches('/');
    (is_category_url(candidate, policy) && candidate == base)
        || candidate.starts_with(&(base.to_owned() + "/page/"))
}

fn classify(text: &str) -> ComponentCategory {
    let lower = text.to_ascii_lowercase();
    if contains_any(&lower, &["bateria", "battery", " lipo", "-lipo"]) {
        ComponentCategory::Battery
    } else if contains_any(&lower, &["helice", "propeller", "power-prop", "-prop-"]) {
        ComponentCategory::Propeller
    } else if contains_any(&lower, &["servo"]) {
        ComponentCategory::Servo
    } else if contains_any(
        &lower,
        &["variador", " esc", "-esc", " bec", "-bec", "ubec", "sbec"],
    ) {
        ComponentCategory::EscBec
    } else if contains_any(&lower, &["motor", "tmotor"]) {
        ComponentCategory::Motor
    } else if contains_any(
        &lower,
        &["receptor", "receiver", "telemetr", "gps", "sensor"],
    ) {
        ComponentCategory::ReceiverTelemetry
    } else if contains_any(&lower, &["fibra", "carbono", "carbon ", "carbon-"]) {
        ComponentCategory::Material
    } else if contains_any(&lower, &["tren", "landing", "patin", "retractil"]) {
        ComponentCategory::LandingGear
    } else if contains_any(
        &lower,
        &["autopilot", "autopiloto", "controller", "electron"],
    ) {
        ComponentCategory::Electronics
    } else {
        ComponentCategory::Other
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn evidence_lines(lines: &[String], title: &str) -> Vec<String> {
    let needles = [
        "peso",
        "weight",
        "masa",
        "mass",
        "volt",
        "kv",
        "corriente",
        "current",
        "capacidad",
        "capacity",
        "dimension",
        "longitud",
        "diametro",
        "diameter",
        "pitch",
        "paso",
        "empuje",
        "thrust",
        "torque",
        "celda",
        "cell",
        "protocolo",
        "protocol",
        "canal",
        "channel",
        "fibra",
        "carbon",
        " mm",
        " ghz",
    ];
    let mut evidence = vec![title.trim().to_owned()];
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if needles.iter().any(|needle| lower.contains(needle))
            && !evidence.iter().any(|seen| seen == line)
        {
            evidence.push(line.chars().take(240).collect());
            if evidence.len() == 16 {
                break;
            }
        }
    }
    evidence
}

fn is_price_line(line: &str) -> bool {
    line.contains('\u{20ac}') || line.to_ascii_uppercase().contains(" EUR")
}

fn is_availability_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    contains_any(
        &lower,
        &[
            "sin stock",
            "out of stock",
            "en stock",
            "in stock",
            "disponible",
        ],
    )
}

fn extract_element_text(html: &str, element: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let opening = format!("<{element}");
    let start = lower.find(&opening)?;
    let content_start = lower[start..].find('>')? + start + 1;
    let closing = format!("</{element}>");
    let end = lower[content_start..].find(&closing)? + content_start;
    visible_lines(&html[content_start..end]).into_iter().next()
}

fn visible_lines(html: &str) -> Vec<String> {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for character in html.chars() {
        match character {
            '<' => {
                in_tag = true;
                text.push('\n');
            }
            '>' => {
                in_tag = false;
                text.push('\n');
            }
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    decode_entities(&text)
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect()
}

fn decode_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}
