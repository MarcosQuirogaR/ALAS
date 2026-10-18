// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Korn technology factor in effect for the main wing (DRG-04).

use super::WingConfig;

impl WingConfig {
    /// Legal range of [`Self::korn_technology_factor_override`].
    pub const KORN_FACTOR_RANGE: std::ops::RangeInclusive<f64> = 0.80..=1.00;

    /// The Korn technology factor in effect: the override when it is finite
    /// and inside [`Self::KORN_FACTOR_RANGE`], else the declared class value.
    /// An out-of-range override is reported by
    /// [`Self::korn_override_error`] and is never used.
    #[must_use]
    pub fn korn_technology_factor(&self) -> f64 {
        match self.korn_technology_factor_override {
            Some(value) if Self::KORN_FACTOR_RANGE.contains(&value) => value,
            _ => self.airfoil_class.korn_technology_factor(),
        }
    }

    /// Why the override is unusable, if it is set and non-finite or outside
    /// [`Self::KORN_FACTOR_RANGE`].
    #[must_use]
    pub fn korn_override_error(&self) -> Option<String> {
        let value = self.korn_technology_factor_override?;
        (!Self::KORN_FACTOR_RANGE.contains(&value)).then(|| {
            format!(
                "Korn technology factor override {value} is outside {}..{}",
                Self::KORN_FACTOR_RANGE.start(),
                Self::KORN_FACTOR_RANGE.end()
            )
        })
    }
}
