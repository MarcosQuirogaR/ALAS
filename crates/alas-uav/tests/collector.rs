// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Offline characterization of sitemap discovery, crawl policy, and evidence extraction.

use std::collections::BTreeMap;
use std::time::Duration;

use alas_uav::collector::{
    built_in_sources, collect, discover_product_urls, robots_allows, ComponentCategory,
    CrawlPolicy, FetchRequest, PageFetcher, Sleeper,
};

const MULTI_SOURCE_PAGES: &str = include_str!("fixtures/multi_source_pages.json");

const SITEMAP: &str = include_str!("fixtures/rc_innovations_sitemap.xml");
const PAGES: &str = include_str!("fixtures/rc_innovations_pages.json");

#[derive(Default)]
struct OfflineFetcher {
    pages: BTreeMap<String, String>,
    requests: Vec<(String, String)>,
}

impl PageFetcher for OfflineFetcher {
    fn fetch(&mut self, request: FetchRequest<'_>) -> Result<String, String> {
        self.requests
            .push((request.url.to_owned(), request.user_agent.to_owned()));
        self.pages
            .get(request.url)
            .cloned()
            .ok_or_else(|| "offline fixture has no response".to_owned())
    }
}

#[derive(Default)]
struct RecordingSleeper(Vec<Duration>);

impl Sleeper for RecordingSleeper {
    fn sleep(&mut self, duration: Duration) {
        self.0.push(duration);
    }
}

fn fixture_fetcher() -> OfflineFetcher {
    let pages: BTreeMap<String, String> = serde_json::from_str(PAGES).expect("page fixture");
    let mut fetcher = OfflineFetcher::default();
    fetcher.pages.insert(
        "https://rc-innovations.es/robots.txt".to_owned(),
        "User-agent: *\nAllow: /\n".to_owned(),
    );
    fetcher.pages.insert(
        "https://rc-innovations.es/sitemap.xml".to_owned(),
        SITEMAP.to_owned(),
    );
    fetcher.pages.extend(pages);
    fetcher
}

#[test]
fn the_offline_crawl_is_bounded_rate_limited_and_source_preserving() {
    let mut fetcher = fixture_fetcher();
    let mut sleeper = RecordingSleeper::default();
    let policy = CrawlPolicy {
        min_request_interval_ms: 37,
        seed_category_urls: vec![
            "https://rc-innovations.es/shop/category/test-components".to_owned()
        ],
        max_category_pages: 3,
        max_product_pages: 20,
        ..CrawlPolicy::default()
    };

    let collected = collect(&mut fetcher, &mut sleeper, &policy).expect("offline crawl");
    assert_eq!(collected.discovered_product_urls, 10);
    assert_eq!(collected.sitemap_product_urls, 9);
    assert_eq!(collected.category_product_urls, 1);
    assert_eq!(collected.category_pages_fetched, 2);
    assert_eq!(collected.products.len(), 10);
    assert_eq!(fetcher.requests.len(), 14);
    assert!(fetcher
        .requests
        .iter()
        .all(|(_, user_agent)| user_agent == &policy.user_agent));
    assert_eq!(sleeper.0, vec![Duration::from_millis(37); 13]);
    assert!(collected.products.iter().all(|product| {
        product
            .source_url
            .starts_with("https://rc-innovations.es/shop/")
            && !product.source_title.is_empty()
            && !product.raw_evidence.is_empty()
            && product.procurement.volatile
    }));

    let categories = collected
        .products
        .iter()
        .map(|product| product.category)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        categories,
        [
            ComponentCategory::Battery,
            ComponentCategory::Electronics,
            ComponentCategory::EscBec,
            ComponentCategory::LandingGear,
            ComponentCategory::Material,
            ComponentCategory::Motor,
            ComponentCategory::Propeller,
            ComponentCategory::ReceiverTelemetry,
            ComponentCategory::Servo,
        ]
        .into_iter()
        .collect()
    );

    let battery = &collected.products[0];
    assert_eq!(battery.procurement.price_text.as_deref(), Some("19,90 EUR"));
    assert_eq!(
        battery.procurement.availability_text.as_deref(),
        Some("En stock")
    );
    assert!(battery
        .raw_evidence
        .iter()
        .any(|line| line.contains("Peso: 180 g")));
}

#[test]
fn robots_uses_the_longest_matching_rule_and_allow_wins_equal_length() {
    let robots = "User-agent: *\nDisallow: /shop/\nAllow: /shop/public/\n";
    assert!(!robots_allows(
        robots,
        "https://rc-innovations.es/shop/private/item"
    ));
    assert!(robots_allows(
        robots,
        "https://rc-innovations.es/shop/public/item"
    ));
}

#[test]
fn robots_applies_a_group_whose_agent_list_includes_the_wildcard() {
    // One group, two agents: the wildcard is not the last `User-agent` line,
    // and its rules still bind this collector.
    let robots = "User-agent: *\nUser-agent: ExampleBot\nDisallow: /shop/\n\n\
                  User-agent: OtherBot\nDisallow: /\n";
    assert!(!robots_allows(
        robots,
        "https://rc-innovations.es/shop/item"
    ));
    assert!(robots_allows(robots, "https://rc-innovations.es/about"));
}

#[test]
fn discovery_removes_queries_duplicates_and_non_product_pages() {
    let sitemap = r#"<urlset>
        <url><loc>https://rc-innovations.es/shop/x-helice-12x6?category=1&amp;page=2</loc></url>
        <url><loc>https://rc-innovations.es/shop/x-helice-12x6</loc></url>
        <url><loc>https://rc-innovations.es/shop/category/helices</loc></url>
        <url><loc>https://rc-innovations.es/contact</loc></url>
    </urlset>"#;
    let urls = discover_product_urls(sitemap, &CrawlPolicy::default()).expect("discovery");
    assert_eq!(urls, ["https://rc-innovations.es/shop/x-helice-12x6"]);
}

#[test]
fn an_allowed_sitemap_cannot_redirect_discovery_to_another_host() {
    let sitemap =
        r#"<urlset><url><loc>https://example.invalid/shop/motor-900kv</loc></url></urlset>"#;
    let error = discover_product_urls(sitemap, &CrawlPolicy::default())
        .expect_err("foreign relevant product must fail closed");
    assert!(error.to_string().contains("outside the allow-list"));
}

#[test]
fn built_in_source_adapters_are_allow_listed_and_have_bounded_caps() {
    let sources = built_in_sources();
    assert_eq!(sources.len(), 14);
    let ids = sources
        .iter()
        .map(|source| source.source_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), sources.len());
    for source in &sources {
        let policy = source.crawl_policy();
        policy.validate().expect("source policy validates");
        assert!(policy.max_product_pages <= 250);
        assert!(policy
            .allowed_hosts
            .iter()
            .all(|host| !host.contains('/') && !host.contains(':')));
        assert!(policy
            .robots_url
            .strip_prefix("https://")
            .is_some_and(|url| url.starts_with(&policy.allowed_hosts[0])));
    }
    let robart = sources
        .iter()
        .find(|source| source.source_id == "robart")
        .expect("Robart source");
    assert_eq!(robart.publisher, "Robart Manufacturing");
    assert!(robart
        .product_path_fragments
        .iter()
        .any(|fragment| fragment == "/products/"));
}

#[test]
fn generic_source_scope_discovers_manufacturer_product_paths() {
    let source = built_in_sources()
        .into_iter()
        .find(|source| source.source_id == "t-motor")
        .expect("T-MOTOR source");
    let policy = source.crawl_policy();
    let sitemap = r#"<urlset>
        <url><loc>https://store.tmotor.com/product/ax435-b-kv220-fixed-wing-motor.html</loc></url>
        <url><loc>https://store.tmotor.com/categorys/tf-series-polymer-folding-propeller</loc></url>
        <url><loc>https://store.tmotor.com/product/other-camera.html</loc></url>
    </urlset>"#;
    let urls = discover_product_urls(sitemap, &policy).expect("T-MOTOR discovery");
    assert_eq!(
        urls,
        ["https://store.tmotor.com/product/ax435-b-kv220-fixed-wing-motor.html"]
    );
}

#[test]
fn saved_multi_source_pages_preserve_title_evidence_and_volatile_procurement() {
    let pages: BTreeMap<String, String> =
        serde_json::from_str(MULTI_SOURCE_PAGES).expect("multi-source page fixture");
    for (url, html) in pages {
        let product = alas_uav::collector::parse_product_page(&url, &html)
            .expect("saved manufacturer page parses");
        assert_eq!(product.source_url, url);
        assert!(!product.source_title.is_empty());
        assert!(!product.raw_evidence.is_empty());
        assert!(product.procurement.volatile);
        assert!(product.procurement.price_text.is_none());
    }
}
