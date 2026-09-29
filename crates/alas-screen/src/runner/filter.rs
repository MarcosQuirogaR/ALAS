// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airfoil-name wildcard filtering.

/// Simple glob / wildcard match for `*` and `?`.
pub(super) fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = None;
    let mut star_ti = 0;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Restrict airfoil `names` to those matching `pattern` (case-insensitive substring or glob).
pub fn filter_names(names: &[&str], pattern: &str) -> Vec<String> {
    let tokens: Vec<String> = pattern
        .split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    if tokens.is_empty() {
        return names.iter().map(|&s| s.to_string()).collect();
    }

    let mut out = Vec::new();
    for &name in names {
        let low = name.to_lowercase();
        for tok in &tokens {
            let matches = if tok.contains('*') || tok.contains('?') {
                glob_match(tok, &low)
            } else {
                low.contains(tok)
            };
            if matches {
                out.push(name.to_string());
                break;
            }
        }
    }
    out
}
