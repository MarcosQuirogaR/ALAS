// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Errors reported by the CPACS 3.5 XML reader.

use thiserror::Error;

/// A failure while reading or validating a CPACS document; diagnostic payload fields are named by the CPACS path or value they describe.
#[allow(missing_docs)]
#[derive(Debug, Error)]
pub enum CpacsReadError {
    /// The input file could not be read.
    #[error("could not read CPACS file: {0}")]
    Io(#[from] std::io::Error),
    /// The input is not well-formed XML.
    #[error("invalid CPACS XML: {0}")]
    Xml(#[from] roxmltree::Error),
    /// The document root is not the CPACS element.
    #[error("CPACS root must be <cpacs>, found <{found}>")]
    InvalidRoot { found: String },
    /// A required CPACS element is absent.
    #[error("missing CPACS element at {path}")]
    MissingElement { path: String },
    /// A required CPACS attribute is absent.
    #[error("missing CPACS attribute {attribute:?} at {path}")]
    MissingAttribute { path: String, attribute: String },
    /// An element or attribute contains only whitespace.
    #[error("empty CPACS value at {path}")]
    EmptyValue { path: String },
    /// A scalar does not contain a finite number.
    #[error("invalid CPACS number {value:?} at {path}")]
    InvalidNumber { path: String, value: String },
    /// A point list has incompatible coordinate arrays.
    #[error("invalid CPACS point list at {path}: {reason}")]
    InvalidPointList { path: String, reason: String },
    /// A UID is reused in the document-wide UID namespace.
    #[error("duplicate CPACS UID {uid:?} at {path}")]
    DuplicateUid { uid: String, path: String },
    /// A UID reference does not resolve to a valid declaration.
    #[error("malformed CPACS reference at {path}: UID {target_uid:?} is not declared")]
    MalformedReference { path: String, target_uid: String },
    /// No CPACS schema version was declared.
    #[error("CPACS document does not declare a schema version")]
    MissingVersion,
    /// The document declares a schema version this reader does not support.
    #[error("unsupported CPACS version {found:?}; only 3.5 is supported")]
    UnsupportedVersion { found: String },
}
