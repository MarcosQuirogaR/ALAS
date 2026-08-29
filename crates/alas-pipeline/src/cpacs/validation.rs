// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! UID registration and reference checks for imported CPACS aircraft data.

use std::collections::{HashMap, HashSet};

use super::model::{CpacsDocument, CpacsFuselage, CpacsSegment, CpacsWing};
use super::reader::CpacsReadError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UidKind {
    Aircraft,
    Fuselage,
    FuselageSection,
    FuselageElement,
    Wing,
    WingSection,
    WingElement,
    Segment,
    Engine,
    EnginePosition,
    FuselageProfile,
    WingAirfoil,
}

pub(super) fn validate_document(document: &CpacsDocument) -> Result<(), CpacsReadError> {
    let mut declarations = HashMap::new();
    register(
        &mut declarations,
        &document.aircraft.uid,
        "cpacs/vehicles/aircraft/model",
        UidKind::Aircraft,
    )?;
    for (index, wing) in document.aircraft.wings.iter().enumerate() {
        register_wing(
            &mut declarations,
            wing,
            &format!("cpacs/vehicles/aircraft/model/wings/wing[{index}]"),
        )?;
    }
    for (index, fuselage) in document.aircraft.fuselages.iter().enumerate() {
        register_fuselage(
            &mut declarations,
            fuselage,
            &format!("cpacs/vehicles/aircraft/model/fuselages/fuselage[{index}]"),
        )?;
    }
    for (index, engine) in document.aircraft.engine_positions.iter().enumerate() {
        let path = format!("cpacs/vehicles/aircraft/model/engines/engine[{index}]");
        register(
            &mut declarations,
            &engine.uid,
            &path,
            UidKind::EnginePosition,
        )?;
    }
    for (index, engine) in document.engines.iter().enumerate() {
        register(
            &mut declarations,
            &engine.uid,
            &format!("cpacs/vehicles/engines/engine[{index}]"),
            UidKind::Engine,
        )?;
    }
    for (index, profile) in document.fuselage_profiles.iter().enumerate() {
        register(
            &mut declarations,
            &profile.uid,
            &format!("cpacs/vehicles/profiles/fuselageProfiles/fuselageProfile[{index}]"),
            UidKind::FuselageProfile,
        )?;
    }
    for (index, airfoil) in document.wing_airfoils.iter().enumerate() {
        register(
            &mut declarations,
            &airfoil.uid,
            &format!("cpacs/vehicles/profiles/wingAirfoils/wingAirfoil[{index}]"),
            UidKind::WingAirfoil,
        )?;
    }
    validate_references(document, &declarations)
}

fn register_wing(
    declarations: &mut HashMap<String, UidKind>,
    wing: &CpacsWing,
    path: &str,
) -> Result<(), CpacsReadError> {
    register(declarations, &wing.uid, path, UidKind::Wing)?;
    for (index, section) in wing.sections.iter().enumerate() {
        let section_path = format!("{path}/sections/section[{index}]");
        register(
            declarations,
            &section.uid,
            &section_path,
            UidKind::WingSection,
        )?;
        for (element_index, element) in section.elements.iter().enumerate() {
            register(
                declarations,
                &element.uid,
                &format!("{section_path}/elements/element[{element_index}]"),
                UidKind::WingElement,
            )?;
        }
    }
    for (index, segment) in wing.segments.iter().enumerate() {
        register(
            declarations,
            &segment.uid,
            &format!("{path}/segments/segment[{index}]"),
            UidKind::Segment,
        )?;
    }
    Ok(())
}

fn register_fuselage(
    declarations: &mut HashMap<String, UidKind>,
    fuselage: &CpacsFuselage,
    path: &str,
) -> Result<(), CpacsReadError> {
    register(declarations, &fuselage.uid, path, UidKind::Fuselage)?;
    for (index, section) in fuselage.sections.iter().enumerate() {
        let section_path = format!("{path}/sections/section[{index}]");
        register(
            declarations,
            &section.uid,
            &section_path,
            UidKind::FuselageSection,
        )?;
        for (element_index, element) in section.elements.iter().enumerate() {
            register(
                declarations,
                &element.uid,
                &format!("{section_path}/elements/element[{element_index}]"),
                UidKind::FuselageElement,
            )?;
        }
    }
    for (index, segment) in fuselage.segments.iter().enumerate() {
        register(
            declarations,
            &segment.uid,
            &format!("{path}/segments/segment[{index}]"),
            UidKind::Segment,
        )?;
    }
    Ok(())
}

fn register(
    declarations: &mut HashMap<String, UidKind>,
    uid: &str,
    path: &str,
    kind: UidKind,
) -> Result<(), CpacsReadError> {
    if declarations.insert(uid.to_owned(), kind).is_some() {
        return Err(CpacsReadError::DuplicateUid {
            uid: uid.to_owned(),
            path: path.to_owned(),
        });
    }
    Ok(())
}

fn validate_references(
    document: &CpacsDocument,
    declarations: &HashMap<String, UidKind>,
) -> Result<(), CpacsReadError> {
    for (index, wing) in document.aircraft.wings.iter().enumerate() {
        let path = format!("cpacs/vehicles/aircraft/model/wings/wing[{index}]");
        validate_optional_reference(
            declarations,
            wing.parent_uid.as_deref(),
            &format!("{path}/parentUID"),
        )?;
        let local_elements = wing
            .sections
            .iter()
            .flat_map(|section| section.elements.iter().map(|element| element.uid.as_str()))
            .collect::<HashSet<_>>();
        for (section_index, section) in wing.sections.iter().enumerate() {
            for (element_index, element) in section.elements.iter().enumerate() {
                let reference_path = format!(
                    "{path}/sections/section[{section_index}]/elements/element[{element_index}]/airfoilUID"
                );
                require_kind(
                    declarations,
                    &element.airfoil_uid,
                    &reference_path,
                    UidKind::WingAirfoil,
                )?;
            }
        }
        validate_segments(
            &wing.segments,
            &local_elements,
            &path,
            declarations,
            UidKind::WingElement,
        )?;
    }
    for (index, fuselage) in document.aircraft.fuselages.iter().enumerate() {
        let path = format!("cpacs/vehicles/aircraft/model/fuselages/fuselage[{index}]");
        validate_optional_reference(
            declarations,
            fuselage.parent_uid.as_deref(),
            &format!("{path}/parentUID"),
        )?;
        let local_elements = fuselage
            .sections
            .iter()
            .flat_map(|section| section.elements.iter().map(|element| element.uid.as_str()))
            .collect::<HashSet<_>>();
        for (section_index, section) in fuselage.sections.iter().enumerate() {
            for (element_index, element) in section.elements.iter().enumerate() {
                let reference_path = format!(
                    "{path}/sections/section[{section_index}]/elements/element[{element_index}]/profileUID"
                );
                require_kind(
                    declarations,
                    &element.profile_uid,
                    &reference_path,
                    UidKind::FuselageProfile,
                )?;
            }
        }
        validate_segments(
            &fuselage.segments,
            &local_elements,
            &path,
            declarations,
            UidKind::FuselageElement,
        )?;
    }
    for (index, engine) in document.aircraft.engine_positions.iter().enumerate() {
        let path = format!("cpacs/vehicles/aircraft/model/engines/engine[{index}]");
        require_kind(
            declarations,
            &engine.engine_uid,
            &format!("{path}/engineUID"),
            UidKind::Engine,
        )?;
        require_any(
            declarations,
            &engine.parent_uid,
            &format!("{path}/parentUID"),
        )?;
    }
    Ok(())
}

fn validate_segments(
    segments: &[CpacsSegment],
    local_elements: &HashSet<&str>,
    parent_path: &str,
    declarations: &HashMap<String, UidKind>,
    element_kind: UidKind,
) -> Result<(), CpacsReadError> {
    for (index, segment) in segments.iter().enumerate() {
        let path = format!("{parent_path}/segments/segment[{index}]");
        require_kind(
            declarations,
            &segment.from_element_uid,
            &format!("{path}/fromElementUID"),
            element_kind,
        )?;
        require_kind(
            declarations,
            &segment.to_element_uid,
            &format!("{path}/toElementUID"),
            element_kind,
        )?;
        if !local_elements.contains(segment.from_element_uid.as_str()) {
            return Err(malformed_reference(
                format!("{path}/fromElementUID"),
                &segment.from_element_uid,
            ));
        }
        if !local_elements.contains(segment.to_element_uid.as_str()) {
            return Err(malformed_reference(
                format!("{path}/toElementUID"),
                &segment.to_element_uid,
            ));
        }
    }
    Ok(())
}

fn validate_optional_reference(
    declarations: &HashMap<String, UidKind>,
    uid: Option<&str>,
    path: &str,
) -> Result<(), CpacsReadError> {
    if let Some(uid) = uid {
        require_any(declarations, uid, path)?;
    }
    Ok(())
}

fn require_any(
    declarations: &HashMap<String, UidKind>,
    uid: &str,
    path: &str,
) -> Result<(), CpacsReadError> {
    if declarations.contains_key(uid) {
        Ok(())
    } else {
        Err(malformed_reference(path.to_owned(), uid))
    }
}

fn require_kind(
    declarations: &HashMap<String, UidKind>,
    uid: &str,
    path: &str,
    expected: UidKind,
) -> Result<(), CpacsReadError> {
    match declarations.get(uid).copied() {
        Some(kind) if kind == expected => Ok(()),
        Some(_) | None => Err(malformed_reference(path.to_owned(), uid)),
    }
}

fn malformed_reference(path: String, target_uid: &str) -> CpacsReadError {
    CpacsReadError::MalformedReference {
        path,
        target_uid: target_uid.to_owned(),
    }
}
