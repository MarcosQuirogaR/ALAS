// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The 125-inch pallets turned a quarter turn, for a main deck whose section
//! takes their long side across the aircraft.

use super::{UldType, PAG, PAP108, PMC};

/// A pallet turned a quarter turn on the deck: the same pallet and type code,
/// its long side across the aircraft.
const fn turned(pallet: UldType) -> UldType {
    UldType {
        length: pallet.width,
        width: pallet.length,
        top_width: pallet.length,
        ..pallet
    }
}
const PAG_TURNED: UldType = turned(PAG);
const PMC_TURNED: UldType = turned(PMC);
const PAP108_TURNED: UldType = turned(PAP108);

/// Main-deck formats the physical selector considers when the configuration
/// names none: the 96- and 88-by-125-inch pallets, lengthwise and turned. The
/// 88-by-108-inch pallet is not among them: it holds less per square metre of
/// deck than it counts positions, so a capacity selector would take it everywhere;
/// an aircraft whose published arrangement uses it names it (`main_deck_uld`).
pub static MAIN_DECK_AUTO_CANDIDATES: [&UldType; 4] = [&PMC, &PMC_TURNED, &PAG, &PAG_TURNED];

/// The turned footprint of a 125- or 108-inch pallet, `None` for any other type.
pub fn turned_pallet(uld: &UldType) -> Option<&'static UldType> {
    [
        (&PAG, &PAG_TURNED),
        (&PMC, &PMC_TURNED),
        (&PAP108, &PAP108_TURNED),
    ]
    .into_iter()
    .find(|(lengthwise, _)| lengthwise.code == uld.code)
    .map(|(lengthwise, turned)| {
        if uld.length == lengthwise.length {
            turned
        } else {
            lengthwise
        }
    })
}
