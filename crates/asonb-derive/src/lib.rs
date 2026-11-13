// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Generate value-independent schemas using Serde's serialization names.
//!
//! The expansion records every possible field and variant before seeing a
//! value. Value-dependent Serde layouts are rejected rather than inferred from
//! whichever instance happens to be serialized first. Use the derive re-export
//! in `asonb`; this crate contains only the compiler-facing implementation.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Fields, LitStr, parse_macro_input, parse_quote};

#[derive(Default)]
/// Serialization-side metadata collected from one container, variant, or field.
struct Attributes {
    /// Explicit serialize name, taking precedence over a casing rule.
    name: Option<String>,
    /// rename_all rule for the container's fields or enumeration's variants.
    rule: Option<String>,
    /// An enum-level rename_all_fields rule for named variant payloads.
    field_rule: Option<String>,
    /// Whether this item is omitted from serialization and therefore its schema.
    skip: bool,
    /// Serde with/serialize_with path, recognized or requiring an override.
    adapter: Option<String>,
    /// User function that builds the actual wire schema for custom serialization.
    override_path: Option<syn::Path>,
    /// Whether a named wrapper uses its sole serialized field's representation.
    transparent: bool,
}

/// Reads supported metadata while consuming unrelated Serde options safely.
/// Only serialization names matter: deserialize-only settings do not describe
/// bytes produced by this writer. Layouts that vary by value are rejected here.
fn attributes(attrs: &[Attribute]) -> syn::Result<Attributes> {
    let mut output = Attributes::default();
    for attr in attrs {
        if attr.path().is_ident("asonb") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("schema_with") {
                    output.override_path = Some(meta.value()?.parse::<LitStr>()?.parse()?);
                    Ok(())
                } else {
                    Err(meta.error("unsupported ASONB attribute; use schema_with = \"path\""))
                }
            })?;
        }
        if !attr.path().is_ident("serde") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("flatten")
                || meta.path.is_ident("skip_serializing_if")
                || meta.path.is_ident("untagged")
                || meta.path.is_ident("tag")
                || meta.path.is_ident("content")
            {
                return Err(meta.error("this Serde representation has no fixed ASONB schema"));
            }
            if meta.path.is_ident("rename")
                || meta.path.is_ident("rename_all")
                || meta.path.is_ident("rename_all_fields")
            {
                let mut name = None;
                if meta.input.peek(syn::Token![=]) {
                    name = Some(meta.value()?.parse::<LitStr>()?.value());
                } else {
                    // Serde allows separate serialize and deserialize names.
                    // Consume both, but retain only the outgoing wire name.
                    meta.parse_nested_meta(|nested| {
                        let value = nested.value()?.parse::<LitStr>()?.value();
                        if nested.path.is_ident("serialize") {
                            name = Some(value);
                        }
                        Ok(())
                    })?;
                }
                if meta.path.is_ident("rename") {
                    output.name = name;
                } else if meta.path.is_ident("rename_all_fields") {
                    output.field_rule = name;
                } else {
                    output.rule = name;
                }
            } else if meta.path.is_ident("skip") || meta.path.is_ident("skip_serializing") {
                output.skip = true;
            } else if meta.path.is_ident("transparent") {
                output.transparent = true;
            } else if meta.path.is_ident("with") || meta.path.is_ident("serialize_with") {
                output.adapter = Some(meta.value()?.parse::<LitStr>()?.value());
            } else if meta.input.peek(syn::Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                meta.parse_nested_meta(|nested| {
                    if nested.input.peek(syn::Token![=]) {
                        let _: syn::Expr = nested.value()?.parse()?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })?;
    }
    Ok(output)
}

/// Matches Serde's casing rules, including its different field/variant rules.
/// This deliberately does not use a general word-splitting library: Serde turns
/// HTTPServer into h_t_t_p_server, not the more usual http_server.
fn renamed(name: &str, rule: Option<&str>, variant: bool) -> syn::Result<String> {
    let snake = || {
        let mut output = String::new();
        for (index, character) in name.chars().enumerate() {
            if index != 0 && character.is_ascii_uppercase() {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        }
        output
    };
    let pascal = || {
        let mut output = String::new();
        let mut upper = true;
        for character in name.chars() {
            if character == '_' {
                upper = true;
            } else {
                output.push(if upper {
                    character.to_ascii_uppercase()
                } else {
                    character
                });
                upper = false;
            }
        }
        output
    };
    let camel = |value: String| {
        let mut characters = value.chars();
        let mut output = String::new();
        if let Some(first) = characters.next() {
            output.push(first.to_ascii_lowercase());
        }
        output.extend(characters);
        output
    };
    Ok(match rule {
        None => name.to_owned(),
        Some("lowercase") => {
            if variant {
                name.to_ascii_lowercase()
            } else {
                name.to_owned()
            }
        }
        Some("UPPERCASE") => name.to_ascii_uppercase(),
        Some("PascalCase") => {
            if variant {
                name.to_owned()
            } else {
                pascal()
            }
        }
        Some("camelCase") => camel(if variant { name.to_owned() } else { pascal() }),
        Some("snake_case") => {
            if variant {
                snake()
            } else {
                name.to_owned()
            }
        }
        Some("SCREAMING_SNAKE_CASE") => {
            if variant {
                snake().to_ascii_uppercase()
            } else {
                name.to_ascii_uppercase()
            }
        }
        Some("kebab-case") => {
            if variant {
                snake().replace('_', "-")
            } else {
                name.replace('_', "-")
            }
        }
        Some("SCREAMING-KEBAB-CASE") => {
            if variant {
                snake().replace('_', "-").to_ascii_uppercase()
            } else {
                name.replace('_', "-").to_ascii_uppercase()
            }
        }
        Some(_) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "unsupported rename_all rule",
            ));
        }
    })
}

/// Generates an expression for a field's wire type, honoring explicit overrides.
/// Known adapters bypass the Rust field type because their Serde representation
/// is intentionally different (for example chrono text versus binary DateTime).
fn field_type(
    field: &syn::Field,
    attrs: &Attributes,
    root: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    if let Some(path) = &attrs.override_path {
        return Ok(quote!(#path(builder)?));
    }
    if let Some(adapter) = &attrs.adapter {
        return match adapter.as_str() {
            "serde_bytes" => Ok(quote!(#root::Type::Bytes(#root::Encoding::Regular))),
            "asonb::date_time" | "crate::date_time" => Ok(quote!(#root::Type::DateTime)),
            "asonb::array" | "crate::array" => {
                let syn::Type::Array(array) = &field.ty else {
                    return Err(syn::Error::new_spanned(
                        field,
                        "the asonb::array adapter requires a fixed-size array field",
                    ));
                };
                let item = &array.elem;
                Ok(quote!(#root::Type::List(
                    #root::Encoding::Regular,
                    Box::new(builder.build::<#item>()?)
                )))
            }
            _ => Err(syn::Error::new_spanned(
                field,
                "custom serialization requires #[asonb(schema_with = \"path\")]",
            )),
        };
    }
    let ty = &field.ty;
    Ok(quote!(builder.build::<#ty>()?))
}

/// Generates ordered child expressions, dropping skipped fields before counting.
/// Named fields become Field descriptions; unnamed positions remain bare Types.
fn fields(
    fields: &Fields,
    rule: Option<&str>,
    root: &proc_macro2::TokenStream,
) -> syn::Result<Vec<proc_macro2::TokenStream>> {
    let mut output = Vec::new();
    for field in fields {
        let attrs = attributes(&field.attrs)?;
        if attrs.skip {
            continue;
        }
        let ty = field_type(field, &attrs, root)?;
        if let Some(ident) = &field.ident {
            // A raw Rust identifier such as r#type is just "type" on the wire.
            let name = attrs.name.unwrap_or(renamed(
                ident.to_string().trim_start_matches("r#"),
                rule,
                false,
            )?);
            output.push(quote!(#root::Field::new(#name,#ty)));
        } else {
            output.push(ty);
        }
    }
    Ok(output)
}

/// Builds the Schema impl while preserving lifetimes, const generics, and bounds.
/// Generated child calls go through SchemaBuilder so reuse and recursion checks
/// work the same way for handwritten and derived implementations.
fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    // A dependency may be renamed in Cargo.toml. Resolve its actual Rust path
    // instead of assuming every downstream crate imports it as `asonb`.
    let root = match proc_macro_crate::crate_name("asonb") {
        Ok(proc_macro_crate::FoundCrate::Itself) => quote!(::asonb),
        Ok(proc_macro_crate::FoundCrate::Name(name)) => {
            let name = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#name)
        }
        Err(error) => return Err(syn::Error::new_spanned(&input.ident, error.to_string())),
    };
    let attrs = attributes(&input.attrs)?;
    let ident = &input.ident;
    let name = attrs.name.unwrap_or_else(|| ident.to_string());
    let mut generics = input.generics.clone();
    for param in generics.type_params_mut() {
        param.bounds.push(parse_quote!(#root::Schema));
    }
    let body = match &input.data {
        Data::Struct(data) => {
            if !matches!(data.fields, Fields::Named(_)) {
                return Err(syn::Error::new_spanned(
                    ident,
                    "unit, newtype, and tuple structs are unsupported by ASONB",
                ));
            }
            let parts = fields(&data.fields, attrs.rule.as_deref(), &root)?;
            if attrs.transparent {
                // Serde removes the wrapper entirely. Exactly one serialized
                // field must survive; skipped fields do not affect that shape.
                let active = data
                    .fields
                    .iter()
                    .filter_map(|field| match attributes(&field.attrs) {
                        Ok(attrs) if !attrs.skip => Some(Ok((field, attrs))),
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    })
                    .collect::<syn::Result<Vec<_>>>()?;
                if active.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "transparent Schema requires one serialized field",
                    ));
                }
                let ty = field_type(active[0].0, &active[0].1, &root)?;
                quote!(#ty)
            } else {
                match &data.fields {
                    Fields::Named(_) => quote!(#root::Type::Object(vec![#(#parts),*])),
                    _ => unreachable!(),
                }
            }
        }
        Data::Enum(data) => {
            let mut variants = Vec::new();
            for variant in &data.variants {
                let variant_attrs = attributes(&variant.attrs)?;
                if variant_attrs.skip {
                    continue;
                }
                let name = variant_attrs.name.unwrap_or(renamed(
                    &variant.ident.to_string(),
                    attrs.rule.as_deref(),
                    true,
                )?);
                let parts = fields(
                    &variant.fields,
                    variant_attrs
                        .rule
                        .as_deref()
                        .or(attrs.field_rule.as_deref()),
                    &root,
                )?;
                variants.push(match &variant.fields {
                    Fields::Unit => quote!(#root::Variant::Unit(#name.into())),
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 && parts.len() == 1 => {
                        let ty = &parts[0];
                        quote!(#root::Variant::Newtype(#name.into(),#ty))
                    }
                    Fields::Unnamed(_) => {
                        if parts.len() < 2 {
                            return Err(syn::Error::new_spanned(
                                &variant.ident,
                                "tuple-like variants require at least two serialized elements",
                            ));
                        }
                        quote!(#root::Variant::Tuple(#name.into(),vec![#(#parts),*]))
                    }
                    Fields::Named(_) => {
                        quote!(#root::Variant::Struct(#name.into(),vec![#(#parts),*]))
                    }
                });
            }
            quote!(#root::Type::Enum(#name.into(),vec![#(#variants),*]))
        }
        Data::Union(_) => {
            return Err(syn::Error::new_spanned(
                ident,
                "unions do not have an ASONB schema",
            ));
        }
    };
    // A direct child/override expression already returns Result and ends in ?.
    // Remove that outer ? rather than wrapping it as Ok(call()?), while ordinary
    // Type constructors need Ok(...). Parsing avoids fragile token-string edits.
    let result = match syn::parse2::<syn::Expr>(body)? {
        syn::Expr::Try(expression) => {
            let expression = expression.expr;
            quote!(#expression)
        }
        expression => quote!(Ok(#expression)),
    };
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    Ok(
        quote!(impl #impl_generics #root::Schema for #ident #ty_generics #where_clause {
            /// Builds this type's serialization schema without inspecting a value.
            fn schema(builder:&mut #root::SchemaBuilder)->Result<#root::Type,#root::AsonbError>{#result}
        }),
    )
}

#[proc_macro_derive(Schema, attributes(serde, asonb))]
/// Derives a value-independent `asonb::Schema` implementation.
///
/// Invoke this through `#[derive(asonb::Schema)]` alongside the Serde traits
/// needed by the application. The generated implementation describes every
/// serialized field and variant without inspecting a runtime value; it does
/// not implement `Serialize` or `Deserialize`.
///
/// # Supported Rust Forms
///
/// Named structs become Objects. Enumerations can have unit, newtype, tuple,
/// and named-field variants. Named `#[serde(transparent)]` structs use their
/// single serialized field's representation, including its adapter or schema
/// override. Unit, newtype, and tuple structs and unions are rejected.
///
/// Lifetimes, const generics, and existing where clauses are preserved. A
/// `Schema` bound is added to each generic type parameter. Successful macro
/// expansion does not itself validate the completed schema: empty Objects,
/// invalid ASON names, recursive types, and other invalid definitions are
/// rejected when `DocumentSchema::of` or a writer builds the schema.
///
/// # Serde Attributes
///
/// The derive follows serialization-side `rename`, `rename_all`,
/// `rename_all_fields`, `skip`, and `skip_serializing` settings. Fields and
/// variants skipped for serialization are absent from the schema; skipping a
/// variant does not permit serializing that variant later. Deserialize-only
/// naming and default policies do not change the writer's schema.
///
/// Recognized field adapters are `serde_bytes`, `asonb::array`, and
/// `asonb::date_time` (also `crate::array` and `crate::date_time` within the main
/// crate). Other `with` or `serialize_with` paths require an explicit field
/// schema override. Adapter paths are recognized by spelling, so aliases also
/// need an override.
///
/// Conditional skips (`skip_serializing_if`), `flatten`, untagged enums, and
/// internally or adjacently tagged enums (`tag` or `content`) are rejected
/// during macro expansion rather than guessed from one serialized value.
///
/// # Custom Field Schemas
///
/// Add `#[asonb(schema_with = "path")]` to a field whose custom Serde encoding
/// differs from its Rust type. The function at `path` must take
/// `&mut asonb::SchemaBuilder` and return `Result<asonb::Type, asonb::AsonbError>`.
/// Use the supplied builder for child types, and describe exactly the encoding
/// emitted by the field's serializer. An override takes precedence over a
/// recognized adapter and need not inspect or construct a field value.
///
/// ```text
/// fn flag_schema(_: &mut asonb::SchemaBuilder)
///     -> Result<asonb::Type, asonb::AsonbError>
/// {
///     Ok(asonb::Type::U8)
/// }
///
/// #[derive(serde::Serialize, asonb::Schema)]
/// struct Flags {
///     #[serde(serialize_with = "serialize_flag")]
///     #[asonb(schema_with = "flag_schema")]
///     enabled: bool,
/// }
/// // serialize_flag must write a u8, not Serde's normal Boolean representation.
/// ```
///
/// # Examples
///
/// In a crate depending on `asonb`, use the re-exported derive as follows
/// (a compiling version is exercised by the main crate's documentation tests):
///
/// ```text
/// #[derive(serde::Serialize, serde::Deserialize, asonb::Schema)]
/// struct Reading {
///     sensor_id: u32,
///     temperature: f64,
/// }
/// let bytes = asonb::ser_to_bytes(&Reading { sensor_id: 7, temperature: 21.5 })?;
/// ```
pub fn derive_schema(input: TokenStream) -> TokenStream {
    expand(parse_macro_input!(input as DeriveInput))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// Checks Serde-compatible casing and rejection of value-dependent fields.
    fn test_attributes_and_names() {
        assert_eq!(
            renamed("my_field", Some("camelCase"), false).unwrap(),
            "myField"
        );
        assert_eq!(
            renamed("MyVariant", Some("snake_case"), true).unwrap(),
            "my_variant"
        );
        assert_eq!(
            renamed("HTTPServer", Some("snake_case"), true).unwrap(),
            "h_t_t_p_server"
        );
        let field: syn::Field = syn::Field::parse_named
            .parse2(quote!(#[serde(skip_serializing_if="Option::is_none")] value:Option<i32>))
            .unwrap();
        assert!(attributes(&field.attrs).is_err());
    }
    use syn::parse::Parser;
}
