// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parsing of the `#[config(...)]` attribute.
//!
//! The attribute is where a field says the things its type cannot: what to
//! call it, what it is for, what range it accepts, and whether the interface
//! should show it at all. Everything here is read at expansion time and
//! becomes constants; nothing in this file runs in a built program.

use syn::spanned::Spanned;

/// Everything one field's `#[config(...)]` attribute said.
#[derive(Default)]
pub struct FieldConfig {
    /// The field is not part of the configuration surface at all.
    pub skip: bool,
    /// The field's type is itself a `ConfigNode` and nests.
    pub nested: bool,
    /// The field is configuration but the interface does not show it.
    pub hidden: bool,
    /// The field is shown only when advanced settings are revealed.
    pub advanced: bool,
    pub label: Option<syn::LitStr>,
    pub unit: Option<syn::LitStr>,
    pub help: Option<syn::LitStr>,
    /// Kept as literals rather than numbers so an integer bound stays an
    /// integer: a field counting seats has a minimum of `0`, not of `0.0`,
    /// and the two are different values once the schema is serialized.
    pub min: Option<syn::Lit>,
    pub max: Option<syn::Lit>,
    pub decimals: Option<syn::LitInt>,
    /// Column headings, for a field holding a table of numbers.
    pub columns: Option<Vec<syn::LitStr>>,
    /// The sibling field and value that make this one editable.
    pub readonly_unless: Option<(syn::LitStr, syn::LitStr)>,
    /// Names the list of accepted values, which lives in a crate this one
    /// must not depend on. See `alas_config::OptionSource`.
    pub options: Option<syn::Ident>,
}

impl FieldConfig {
    /// Read the `#[config(...)]` attributes on one field.
    ///
    /// # Errors
    ///
    /// On an unknown key, a malformed value, or a field that states no
    /// metadata at all -- the last of which is the rule CONTRIBUTING.md
    /// describes: a configuration field with neither documentation nor an
    /// explicit `skip` would reach the settings interface as a blank mystery,
    /// so it does not compile.
    pub fn parse(field: &syn::Field) -> syn::Result<Self> {
        let mut parsed = Self::default();
        let mut seen = false;

        for attribute in &field.attrs {
            if !attribute.path().is_ident("config") {
                continue;
            }
            seen = true;
            attribute.parse_nested_meta(|meta| parsed.absorb(&meta))?;
        }

        if !seen {
            return Err(syn::Error::new(
                field.span(),
                "every configuration field needs #[config(help = \"...\")] so the \
                 settings interface has something to show, or #[config(skip)] to \
                 say it is not configuration",
            ));
        }
        if !parsed.skip && parsed.help.is_none() {
            return Err(syn::Error::new(
                field.span(),
                "#[config(...)] here states no `help`; a field the interface \
                 renders with no explanation is a blank mystery to whoever has \
                 to set it. Add `help = \"...\"`, or `skip` if this is not \
                 configuration",
            ));
        }
        if parsed.skip && parsed.nested {
            return Err(syn::Error::new(
                field.span(),
                "`skip` and `nested` contradict each other",
            ));
        }

        Ok(parsed)
    }

    fn absorb(&mut self, meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<()> {
        let path = &meta.path;
        if path.is_ident("skip") {
            self.skip = true;
        } else if path.is_ident("nested") {
            self.nested = true;
        } else if path.is_ident("hidden") {
            self.hidden = true;
        } else if path.is_ident("advanced") {
            self.advanced = true;
        } else if path.is_ident("label") {
            self.label = Some(meta.value()?.parse()?);
        } else if path.is_ident("unit") {
            self.unit = Some(meta.value()?.parse()?);
        } else if path.is_ident("help") {
            self.help = Some(meta.value()?.parse()?);
        } else if path.is_ident("min") {
            self.min = Some(number(meta)?);
        } else if path.is_ident("max") {
            self.max = Some(number(meta)?);
        } else if path.is_ident("decimals") {
            self.decimals = Some(meta.value()?.parse()?);
        } else if path.is_ident("columns") {
            self.columns = Some(string_list(meta)?);
        } else if path.is_ident("readonly_unless") {
            self.readonly_unless = Some(readonly_unless(meta)?);
        } else if path.is_ident("options") {
            self.options = Some(meta.value()?.parse()?);
        } else {
            return Err(meta.error("unknown #[config(...)] key"));
        }
        Ok(())
    }
}

/// A bound, which may be written as an integer or as a float.
fn number(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<syn::Lit> {
    let literal: syn::Lit = meta.value()?.parse()?;
    match literal {
        syn::Lit::Int(_) | syn::Lit::Float(_) => Ok(literal),
        other => Err(syn::Error::new(other.span(), "expected a number")),
    }
}

/// `columns = ["x-station [m]", "radius fraction [0-1]"]`.
fn string_list(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<Vec<syn::LitStr>> {
    let array: syn::ExprArray = meta.value()?.parse()?;
    array
        .elems
        .into_iter()
        .map(|element| match element {
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(text),
                ..
            }) => Ok(text),
            other => Err(syn::Error::new(other.span(), "expected a string literal")),
        })
        .collect()
}

/// `readonly_unless(field = "class_mix_mode", value = "count")`.
///
/// The sibling is named rather than referenced because the condition is
/// resolved by whatever renders the form, against the values it currently
/// holds, and that is not necessarily this struct: upstream resolves the name
/// against the nearest enclosing set of form values.
fn readonly_unless(
    meta: &syn::meta::ParseNestedMeta<'_>,
) -> syn::Result<(syn::LitStr, syn::LitStr)> {
    let mut field = None;
    let mut value = None;
    meta.parse_nested_meta(|inner| {
        if inner.path.is_ident("field") {
            field = Some(inner.value()?.parse()?);
        } else if inner.path.is_ident("value") {
            value = Some(inner.value()?.parse()?);
        } else {
            return Err(inner.error("expected `field` or `value`"));
        }
        Ok(())
    })?;

    match (field, value) {
        (Some(field), Some(value)) => Ok((field, value)),
        _ => Err(meta.error("readonly_unless needs both `field` and `value`")),
    }
}
