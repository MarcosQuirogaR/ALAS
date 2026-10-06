// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Validation of the user-defined and generated-override fuselage sections.

use super::{FuselageConfig, FuselageSection, FuselageSectionError};

impl FuselageConfig {
    /// Validate user-defined fuselage sections without changing their values.
    pub fn validate_custom_sections(&self) -> Result<(), FuselageSectionError> {
        let mut previous = None;
        for (index, section) in self.custom_sections.iter().enumerate() {
            for (field, value) in [
                ("x_fraction", section.x_fraction),
                ("width_m", section.width_m),
                ("height_m", section.height_m),
                ("z_m", section.z_m),
                ("shape", section.shape),
            ] {
                if !value.is_finite() {
                    return Err(FuselageSectionError::NonFinite {
                        index,
                        field,
                        value,
                    });
                }
            }
            if !(0.0..1.0).contains(&section.x_fraction) {
                return Err(FuselageSectionError::XOutOfRange {
                    index,
                    value: section.x_fraction,
                });
            }
            for (field, value) in [("width", section.width_m), ("height", section.height_m)] {
                if value <= 0.0 {
                    return Err(FuselageSectionError::NonPositive {
                        index,
                        field,
                        value,
                    });
                }
            }
            if !(1.0..=50.0).contains(&section.shape) {
                return Err(FuselageSectionError::InvalidShape {
                    index,
                    value: section.shape,
                });
            }
            if let Some(previous) = previous {
                if section.x_fraction <= previous {
                    return Err(FuselageSectionError::InvalidOrder {
                        index,
                        previous,
                        current: section.x_fraction,
                    });
                }
            }
            previous = Some(section.x_fraction);
        }
        Ok(())
    }

    /// Return custom sections in validated nose-to-tail order.
    pub fn custom_sections_sorted(&self) -> Result<Vec<FuselageSection>, FuselageSectionError> {
        self.validate_custom_sections()?;
        let mut sections = self.custom_sections.clone();
        sections.sort_by(|left, right| left.x_fraction.total_cmp(&right.x_fraction));
        Ok(sections)
    }

    /// Validate the optional generated-station override vector.
    pub fn validate_generated_sections(&self) -> Result<(), FuselageSectionError> {
        const GENERATED_STATION_COUNT: usize = 20;
        if self.generated_sections.is_empty() {
            return Ok(());
        }
        if self.generated_sections.len() != GENERATED_STATION_COUNT {
            return Err(FuselageSectionError::GeneratedSectionCount {
                expected: GENERATED_STATION_COUNT,
                actual: self.generated_sections.len(),
            });
        }
        for (index, section) in self.generated_sections.iter().enumerate() {
            for (field, value) in [
                ("x_fraction", section.x_fraction),
                ("width_m", section.width_m),
                ("height_m", section.height_m),
                ("z_m", section.z_m),
                ("shape", section.shape),
            ] {
                if !value.is_finite() {
                    return Err(FuselageSectionError::NonFinite {
                        index,
                        field,
                        value,
                    });
                }
            }
            if !(0.0..=1.0).contains(&section.x_fraction) {
                return Err(FuselageSectionError::XOutOfRange {
                    index,
                    value: section.x_fraction,
                });
            }
            for (field, value) in [("width", section.width_m), ("height", section.height_m)] {
                if value <= 0.0 {
                    return Err(FuselageSectionError::NonPositive {
                        index,
                        field,
                        value,
                    });
                }
            }
            if !(1.0..=50.0).contains(&section.shape) {
                return Err(FuselageSectionError::InvalidShape {
                    index,
                    value: section.shape,
                });
            }
        }
        Ok(())
    }
}
