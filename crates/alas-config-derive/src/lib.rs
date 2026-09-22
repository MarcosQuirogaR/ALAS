// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `#[derive(ConfigNode)]`: the settings interface, generated from the
//! configuration structs themselves.
//!
//! Everything a user can tune lives in a struct in `alas-config`, and the
//! interface that edits those values is not written by hand; it is rendered
//! from a description each struct produces of its own fields. This derive is
//! where that description comes from. The reference implementation builds the
//! same description by walking a dataclass at run time
//! (`alas/sidecar/schema.py`); doing it at expansion time instead means a
//! field with no explanation is a compile error rather than a blank row in a
//! form nobody notices until they have to fill it in.
//!
//! # What a field has to say
//!
//! ```ignore
//! #[derive(ConfigNode)]
//! pub struct DragModelConfig {
//!     #[config(help = "Lumped multiplier applied to the total parasite drag.")]
//!     pub viscous_margin: f64,
//!
//!     #[config(
//!         label = "Max-thickness chordwise location",
//!         unit = "x/c",
//!         min = 0.0,
//!         max = 1.0,
//!         help = "Where the airfoil is thickest, as a fraction of chord.",
//!     )]
//!     pub max_thickness_chordwise_loc: f64,
//! }
//! ```
//!
//! `label` and `unit` are optional: without them both are derived from the
//! field's name by the rule in [`names`], which is what several hundred
//! upstream fields rely on. `help` is not optional, and neither is saying
//! `skip` instead, see CONTRIBUTING.md on why an undocumented field is a
//! defect rather than a shortcut.
//!
//! # Nesting
//!
//! A field whose type is itself a `ConfigNode` is marked `#[config(nested)]`.
//! A derive macro sees types only as syntax and cannot tell `f64` from a
//! struct, so the alternative to one word here is a trait-resolution scheme
//! that reports a missing marker as a page of unsatisfied bounds. The word is
//! better.

mod attr;
mod names;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};

/// Derive the field descriptors the settings interface is generated from.
///
/// See the module documentation for the attribute grammar.
#[proc_macro_derive(ConfigNode, attributes(config))]
pub fn derive_config_node(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: &syn::DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    let fields = named_fields(input)?;

    let mut pushes = Vec::new();
    for field in fields {
        if let Some(push) = field_descriptor(field)? {
            pushes.push(push);
        }
    }

    let type_name = name.to_string();
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics ::alas_config::ConfigNode for #name #type_generics #where_clause {
            fn schema(&self) -> ::alas_config::Node {
                let mut fields = ::std::vec::Vec::new();
                #(#pushes)*
                ::alas_config::Node {
                    type_name: #type_name,
                    fields,
                }
            }
        }
    })
}

fn named_fields(
    input: &syn::DeriveInput,
) -> syn::Result<&syn::punctuated::Punctuated<syn::Field, syn::Token![,]>> {
    match &input.data {
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Named(named),
            ..
        }) => Ok(&named.named),
        _ => Err(syn::Error::new_spanned(
            input,
            "ConfigNode describes a struct with named fields: there is nothing \
             for a form to render in a tuple struct or an enum",
        )),
    }
}

/// The statement that appends one field's descriptor, or `None` when the
/// field is not part of the configuration surface.
fn field_descriptor(field: &syn::Field) -> syn::Result<Option<proc_macro2::TokenStream>> {
    let config = attr::FieldConfig::parse(field)?;
    if config.skip || config.hidden {
        return Ok(None);
    }

    let ident = field.ident.as_ref().ok_or_else(|| {
        syn::Error::new_spanned(field, "ConfigNode needs every field to be named")
    })?;
    let name = ident.to_string();
    let (auto_label, auto_unit) = names::humanize(&name);

    let label = config
        .label
        .as_ref()
        .map_or_else(|| quote!(#auto_label), ToTokens::to_token_stream);
    let unit = config
        .unit
        .as_ref()
        .map_or_else(|| quote!(#auto_unit), ToTokens::to_token_stream);
    let help = &config.help;
    let advanced = config.advanced;

    let entry = if config.nested {
        quote!(::alas_config::Entry::Node(
            ::alas_config::ConfigNode::schema(&self.#ident)
        ))
    } else {
        leaf_entry(ident, &name, &config)
    };

    Ok(Some(quote! {
        fields.push(::alas_config::Field {
            name: #name,
            label: #label,
            unit: #unit,
            help: #help,
            advanced: #advanced,
            entry: #entry,
        });
    }))
}

fn leaf_entry(
    ident: &syn::Ident,
    name: &str,
    config: &attr::FieldConfig,
) -> proc_macro2::TokenStream {
    let min = optional_number(config.min.as_ref());
    let max = optional_number(config.max.as_ref());
    let decimals = match &config.decimals {
        Some(literal) => quote!(::std::option::Option::Some(#literal)),
        None => quote!(::std::option::Option::None),
    };
    let columns = match &config.columns {
        Some(headings) => quote!(::std::option::Option::Some(&[#(#headings),*])),
        None => quote!(::std::option::Option::None),
    };
    let readonly_unless = match &config.readonly_unless {
        Some((sibling, value)) => quote!(::std::option::Option::Some(
            ::alas_config::ReadonlyUnless { field: #sibling, value: #value }
        )),
        None => quote!(::std::option::Option::None),
    };
    let options = match &config.options {
        Some(source) => {
            quote!(::std::option::Option::Some(::alas_config::OptionSource::#source))
        }
        None => quote!(::std::option::Option::None),
    };

    quote! {
        ::alas_config::Entry::Leaf(::alas_config::LeafField {
            kind: ::alas_config::Leaf::kind(&self.#ident, #name),
            value: ::alas_config::Leaf::value(&self.#ident),
            min: #min,
            max: #max,
            decimals: #decimals,
            columns: #columns,
            readonly_unless: #readonly_unless,
            options: #options,
        })
    }
}

/// A bound, carried into the schema with its integer-ness intact.
///
/// `Value::from` resolves the literal's own type, so a bound written `0`
/// becomes an integer and one written `0.0` becomes a float, which is the
/// distinction the reference's metadata makes and the interface reads.
fn optional_number(literal: Option<&syn::Lit>) -> proc_macro2::TokenStream {
    match literal {
        Some(literal) => quote!(::std::option::Option::Some(
            ::alas_config::Number::from(#literal)
        )),
        None => quote!(::std::option::Option::None),
    }
}
