// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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

