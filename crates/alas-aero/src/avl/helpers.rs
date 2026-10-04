// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

pub(super) fn required_value(text: &str, label: &'static str) -> Result<f64, AvlError> {
    optional_value(text, label)?.ok_or(AvlError::MissingValue(label))
}

pub(super) fn optional_value(text: &str, label: &'static str) -> Result<Option<f64>, AvlError> {
    for line in text.lines() {
        let tokens = line.split_whitespace().collect::<Vec<_>>();
        for window in tokens.windows(3) {
            if window[0] == label && window[1] == "=" {
                let normalized = window[2].replace(['D', 'd'], "E");
                let value = normalized
                    .parse::<f64>()
                    .map_err(|_| AvlError::InvalidNumber {
                        label,
                        token: window[2].to_owned(),
                    })?;
                if !value.is_finite() && label != "e" {
                    return Err(AvlError::InvalidNumber {
                        label,
                        token: window[2].to_owned(),
                    });
                }
                return Ok(Some(value));
            }
        }
    }

    // AVL's `MRF` mode retains the same labels in a full-precision,
    // machine-readable layout: values precede a `|` and the corresponding
    // comma-separated labels follow it.  Accepting both forms keeps the
    // parser useful for retained legacy FT files while allowing the product
    // runner to compare SI references without precision loss.
    for line in text.lines() {
        let Some((value_text, label_text)) = line.split_once('|') else {
            continue;
        };
        let labels = label_text.split(',').map(str::trim).collect::<Vec<_>>();
        let Some(index) = labels.iter().position(|candidate| {
            candidate
                .rsplit_once(':')
                .map_or(*candidate, |(_, suffix)| suffix.trim())
                == label
        }) else {
            continue;
        };
        let values = value_text.split_whitespace().collect::<Vec<_>>();
        let Some(token) = values.get(index) else {
            return Err(AvlError::MissingValue(label));
        };
        let normalized = token.replace(['D', 'd'], "E");
        let value = normalized
            .parse::<f64>()
            .map_err(|_| AvlError::InvalidNumber {
                label,
                token: (*token).to_owned(),
            })?;
        if !value.is_finite() && label != "e" {
            return Err(AvlError::InvalidNumber {
                label,
                token: (*token).to_owned(),
            });
        }
        return Ok(Some(value));
    }
    Ok(None)
}

pub(super) fn sanitized_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "ALAS aircraft".to_owned()
    } else {
        sanitized
    }
}
