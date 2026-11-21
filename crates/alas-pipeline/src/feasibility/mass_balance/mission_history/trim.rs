// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The trim-tank control of the flown walk: holding the centre of gravity in a
//! band, and releasing the trim fuel forward.
//!
//! Without a sourced pump rate (`TrimTankConfig::transfer_rate_kg_s` unset) a
//! move is instantaneous, which is an unsourced idealisation and is said so in
//! the event text. With a rate `r` the pump moves at most `r * dt` kg of trim fuel
//! over an interval `dt` (a leg while the control is on, zero at an event), so
//! the band can be left between two legs and recovered during the next one.
//! Within a leg the walk applies the move at the leg's end, not at each instant.

use alas_mass::tanks::FuelState;

use super::Walker;

impl Walker<'_> {
    /// Largest trim-fuel move the pump makes over `dt_s` seconds, kg.
    fn trim_move_limit_kg(&self, dt_s: f64) -> f64 {
        match self.rate_kg_s {
            None => f64::INFINITY,
            Some(rate) => rate * dt_s.max(0.0),
        }
    }

    /// The trim fuel that moves the centre of gravity back to the nearest band
    /// edge, kg; `None` when there is no band or the centre of gravity is inside it.
    fn band_fuel_kg(&self) -> Option<f64> {
        let [min, max] = self.trim_band?;
        let layout = self.inputs.tanks;
        let current = self.pct_mac(&self.properties(&self.state));
        let (target, going_aft) = if current < min {
            (min, true)
        } else if current > max {
            (max, false)
        } else {
            return None;
        };
        let trim_now = self.state.trim_fuel_kg(layout);
        let (mut low, mut high) = if going_aft {
            (trim_now, FuelState::trim_capacity_kg(layout))
        } else {
            (0.0, trim_now)
        };
        // The centre of gravity moves aft as trim fuel is added: bisect on it,
        // whichever edge of the band is the target.
        for _ in 0..48 {
            let middle = 0.5 * (low + high);
            let (state, _) = self.state.with_trim_fuel(layout, middle);
            let pct = self.pct_mac(&self.properties(&state));
            if pct < target {
                low = middle;
            } else {
                high = middle;
            }
        }
        Some(if going_aft { high } else { low })
    }

    /// Move the trim fuel toward its wanted value (the band edge, or zero during
    /// a release) by at most the pump's move over `dt_s` seconds.
    pub(super) fn hold_trim(&mut self, dt_s: f64) {
        let layout = self.inputs.tanks;
        let trim_now = self.state.trim_fuel_kg(layout);
        if self.releasing && trim_now <= 0.5 {
            self.releasing = false;
        }
        let wanted = if self.releasing {
            0.0
        } else {
            match self.band_fuel_kg() {
                Some(wanted) => wanted,
                None => return,
            }
        };
        let delta = wanted - trim_now;
        let limit = self.trim_move_limit_kg(dt_s);
        let target = if delta.abs() <= limit {
            wanted
        } else {
            trim_now + limit.copysign(delta)
        };
        if target == trim_now {
            return;
        }
        let (state, reached) = self.state.with_trim_fuel(layout, target);
        if target == wanted && (reached - wanted).abs() > 0.5 {
            self.notes.push(format!(
                "the trim tank could not reach {wanted:.0} kg (it holds {reached:.0} kg): the centre of gravity stays outside its band"
            ));
        }
        if self.releasing && reached <= 0.5 {
            self.releasing = false;
        }
        self.state = state;
    }

    /// Start the trim control on a band; with a pump rate the fuel moves in later
    /// intervals, without one it moves at once.
    pub(super) fn trim_on(&mut self, band: [f64; 2]) {
        self.trim_band = Some(band);
        self.hold_trim(0.0);
    }

    /// Release the trim control: the trim fuel goes forward, at once without a
    /// pump rate, or at the pump rate over the following intervals with one.
    pub(super) fn trim_off(&mut self) {
        self.trim_band = None;
        if self.rate_kg_s.is_some() {
            self.releasing = true;
            self.hold_trim(0.0);
        } else {
            let (state, _) = self.state.with_trim_fuel(self.inputs.tanks, 0.0);
            self.state = state;
        }
    }

    /// How the trim control's fuel movement is described in the event text.
    pub(super) fn trim_pump_text(&self) -> &'static str {
        match self.rate_kg_s {
            None => "(the fuel moves instantly: no transfer rate is modelled)",
            Some(_) => "(the fuel moves at the sourced transfer rate)",
        }
    }
}
