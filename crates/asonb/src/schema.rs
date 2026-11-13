// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Type descriptions are independent of values, including empty collections
//! and enumeration variants that are not selected by the current value.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    hash::Hash,
    io::Write,
};

use crate::{
    AsonbError,
    binary::{Input, write_all, write_u32},
};

/// A value's wire type, independent of the Rust type used to read or write it.
///
/// Container children describe every possible element or variant, even when a
/// particular value is empty. [`Type::Reference`] reuses a definition from the
/// enclosing [`DocumentSchema`]; it does not represent a shared value or pointer.
/// Definitions are data, not validated constructors: assemble a
/// [`DocumentSchema`] and call [`DocumentSchema::validate`] before relying on
/// names, reference indices, or container arities. Public document readers and
/// writers perform this validation automatically.
///
/// Numeric classes are exact wire types, not conversion requests. A
/// [`crate::Value::I32`] cannot be written with an `I64` schema, even if its value
/// would fit. All multibyte scalar payloads, counts, and indices are little-endian.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// A signed 8-bit integer.
    I8,
    /// A signed 16-bit little-endian integer.
    I16,
    /// A signed 32-bit little-endian integer.
    I32,
    /// A signed 64-bit little-endian integer.
    I64,
    /// An unsigned 8-bit integer.
    U8,
    /// An unsigned 16-bit little-endian integer.
    U16,
    /// An unsigned 32-bit little-endian integer.
    U32,
    /// An unsigned 64-bit little-endian integer.
    U64,
    /// An IEEE 754 single-precision value; writers use `0x7fc00000` for NaN.
    F32,
    /// An IEEE 754 double-precision value; writers use `0x7ff8000000000000` for NaN.
    F64,
    /// A one-byte Boolean: writers use zero/one; readers treat nonzero as true.
    Bool,
    /// A Unicode scalar value stored as a little-endian u32.
    Char,
    /// An eight-byte signed count of whole seconds since the Unix epoch.
    ///
    /// The original UTC offset and fractional seconds are not stored. Dynamic
    /// values support the full `i64` range; the [`crate::date_time`] adapter also
    /// checks that a decoded timestamp fits chrono.
    DateTime,
    /// UTF-8 text with the selected length or chunk framing.
    String(
        /// How the UTF-8 bytes are framed.
        Encoding,
    ),
    /// Raw bytes, distinct from a List of u8 values.
    Bytes(
        /// How the byte payload is framed.
        Encoding,
    ),
    /// A homogeneous sequence, whose type is known even when it is empty.
    List(
        /// How element counts are framed.
        Encoding,
        /// The schema shared by all elements.
        Box<Type>,
    ),
    /// A sequence of key/value pairs; keys and values each have one fixed type.
    ///
    /// The schema does not require sorted or unique keys. Maps have a single
    /// count prefix and do not support MultiChunk framing.
    Map(
        /// Whether the pair count uses u64 (`true`) instead of u32 (`false`).
        bool,
        /// The type of each key.
        Box<Type>,
        /// The type of each value.
        Box<Type>,
    ),
    /// A nonempty sequence with a separate type for each position.
    ///
    /// Payload values have no count prefix: the schema fixes the arity.
    Tuple(
        /// Element types, in payload order.
        Vec<Type>,
    ),
    /// A nonempty set of named fields; values follow schema order on the wire.
    ///
    /// Names occur only in the schema, not before each payload field.
    Object(
        /// Field names and types, in payload order.
        Vec<Field>,
    ),
    /// A named choice among unit, newtype, tuple, or struct variants.
    ///
    /// A value stores a `u32` variant index followed by that variant's payload.
    /// Enumeration and variant names must satisfy [`DocumentSchema::validate`].
    Enum(
        /// The ASON identifier naming this enumeration.
        String,
        /// All possible variants; their positions are their wire indices.
        Vec<Variant>,
    ),
    /// A zero-based index into the enclosing document's reference table.
    ///
    /// Definitions may be reused, but missing entries and cycles are invalid.
    Reference(
        /// The referenced definition's index, not a byte offset.
        u32,
    ),
}

/// Length and chunk framing for Strings, Byte Data, and Lists.
///
/// Lengths count UTF-8 bytes for Strings, raw bytes for Byte Data, and elements
/// for Lists; they never include the prefix itself. All prefixes are
/// little-endian. This choice changes framing, not the logical value or its
/// element type. Ordinary Serde mappings use [`Self::Regular`]; select another
/// form with a dynamic [`Type`] or a custom [`Schema`] implementation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// One `u32` length followed by the entire payload, including a zero length
    /// for an empty value. Counts greater than `u32::MAX` cannot use this form.
    Regular,
    /// One `u64` length followed by the entire payload.
    ///
    /// A wide wire count does not guarantee that the value fits in memory on
    /// the reading platform.
    Long,
    /// Repeated nonzero `u32` chunk lengths and their payloads, then a zero count.
    ///
    /// At the document root, EOF between chunks can also end the value. Nested
    /// values, including the List inside `Result::Ok`, require the zero marker.
    /// EOF inside a count or payload is always truncation. Each String chunk
    /// must be independently valid UTF-8; a character cannot span chunks.
    ///
    /// Dynamic readers concatenate chunks and discard their original
    /// boundaries. The List streaming APIs instead yield individual elements.
    MultiChunk,
}

/// A named field of an Object or an object-like enumeration variant.
///
/// Field order determines payload order. Names must be unique within that
/// Object or variant, valid ASON identifiers, and not reserved ASON keywords;
/// see [`DocumentSchema::validate`]. Creating or editing a field does not run
/// these checks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    /// The field's wire name, after any serialization-side Serde rename.
    ///
    /// This name is stored in the schema and used for named-field decoding.
    pub name: String,
    /// The schema used to read or write the field's value.
    ///
    /// References are resolved through the enclosing [`DocumentSchema`].
    pub ty: Type,
}

impl Field {
    /// Creates a field description without validating it yet.
    ///
    /// `name` becomes the owned wire name; `ty` describes the field's payload.
    /// [`DocumentSchema::validate`] checks identifier syntax, reserved keywords,
    /// duplicate names in the enclosing field list, and the payload type.
    /// This constructor does not fail for an invalid name or type.
    ///
    /// # Examples
    ///
    /// ```
    /// let field = asonb::Field::new("count", asonb::Type::U32);
    /// assert_eq!(field.name, "count");
    /// ```
    pub fn new(name: impl Into<String>, ty: Type) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

/// A named enumeration variant and the shape of its payload.
///
/// The schema uses form tags `0`, `1`, `2`, and `4` for unit, newtype, tuple,
/// and struct variants, respectively. These tags describe the schema; a value
/// instead selects a variant by its position in [`Type::Enum`]'s variant list.
/// Names must be unique within the enumeration and pass
/// [`DocumentSchema::validate`]. Variant order need not match a Rust enum's
/// declaration order because Serde variants are matched by wire name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Variant {
    /// A variant with no payload.
    ///
    /// Only the variant index is written in the value area.
    Unit(
        /// The variant's wire name.
        String,
    ),

    /// A variant with a single payload.
    ///
    /// Unlike ordinary Rust newtype structs, newtype enum variants are supported.
    Newtype(
        /// The variant's wire name.
        String,
        /// The single payload's type.
        Type,
    ),

    /// A variant with a tuple payload.
    ///
    /// Its fixed arity comes from the schema; no count prefix is written.
    Tuple(
        /// The variant's wire name.
        String,
        /// At least two payload types, in order.
        Vec<Type>,
    ),

    /// A variant with a struct payload.
    ///
    /// As with an Object, names are stored in the schema and values follow
    /// field order.
    Struct(
        /// The variant's wire name.
        String,
        /// A nonempty field list, in payload order.
        Vec<Field>,
    ),
}

impl Variant {
    /// Borrows the wire name, regardless of the variant's payload shape.
    ///
    /// This does not resolve a reference or allocate a new string.
    ///
    /// # Examples
    ///
    /// ```
    /// let variant = asonb::Variant::Newtype("Count".into(), asonb::Type::U32);
    /// assert_eq!(variant.name(), "Count");
    /// ```
    pub fn name(&self) -> &str {
        match self {
            Self::Unit(name)
            | Self::Newtype(name, _)
            | Self::Tuple(name, _)
            | Self::Struct(name, _) => name,
        }
    }
}

/// The complete type metadata stored before a document's root payload.
///
/// A schema contains the root type and every reusable definition it references.
/// Reference indices are local to this table; copying a referenced [`Type`] into
/// a different schema without its definitions can change or invalidate it.
/// References share type descriptions, never object identity or payload storage.
///
/// Use [`Self::of`] to derive and validate metadata from a Rust type, or
/// [`Self::new`] and the public fields to assemble it manually. Construction and
/// field mutation do not validate anything; call [`Self::validate`] to check a
/// manually assembled schema. Public document I/O validates schemas as well.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSchema {
    /// Reusable definitions indexed by [`Type::Reference`].
    ///
    /// Entries may themselves contain references. All entries, including unused
    /// ones, must be valid and acyclic. Reordering entries requires updating
    /// every affected reference index.
    pub references: Vec<Type>,

    /// The type of the single root value, possibly a reference into this table.
    pub root: Type,
}

impl DocumentSchema {
    /// Creates a schema with `root` as its value type and an empty reference table.
    ///
    /// No validation happens here. If `root` contains references, fill in
    /// [`Self::references`] before validating or writing the document.
    ///
    /// # Examples
    ///
    /// ```
    /// let schema = asonb::DocumentSchema::new(asonb::Type::I32);
    /// schema.validate()?;
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn new(root: Type) -> Self {
        Self {
            references: Vec::new(),
            root,
        }
    }

    /// Builds and validates the complete schema for Rust type `T`.
    ///
    /// No value is needed: element types and all enum variants come from
    /// [`Schema`]. Repeated compound definitions share reference entries.
    /// The returned root can therefore be a [`Type::Reference`] rather than an
    /// inline compound definition. Each call uses a fresh [`SchemaBuilder`].
    /// This checks the schema, not whether a future Serde value will follow it.
    ///
    /// # Errors
    ///
    /// Returns an error for recursive Rust schemas, errors reported by custom
    /// [`Schema`] implementations, an unrepresentable reference index, or any
    /// invalid definition rejected by [`Self::validate`]. An invalid unselected
    /// enum branch or an unsupported element type in an empty collection still
    /// causes an error.
    ///
    /// # Examples
    ///
    /// ```
    /// let schema = asonb::DocumentSchema::of::<Vec<i32>>()?;
    /// assert_eq!(schema.root,
    ///     asonb::Type::List(asonb::Encoding::Regular, Box::new(asonb::Type::I32)));
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn of<T: Schema + ?Sized>() -> Result<Self, AsonbError> {
        let mut builder = SchemaBuilder::default();
        let root = builder.build::<T>()?;
        let schema = Self {
            references: builder.references,
            root,
        };
        schema.validate()?;
        Ok(schema)
    }

    /// Checks the root and every reference entry, including unused definitions.
    ///
    /// Leaves the schema unchanged. Validation does not impose a size or nesting
    /// quota, check a value's compatibility, or ensure that every declared
    /// count will fit the wire framing when a value is eventually written.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the following rules is violated:
    ///
    /// - References must exist and form an acyclic graph, even in unused entries.
    /// - Field, enumeration, and variant names must be valid ASON identifiers.
    ///   These are not restricted to Rust's identifier rules.
    /// - The exact names `true`, `false`, `NaN`, `NaN_f32`, `NaN_f64`, `Inf`,
    ///   `Inf_f32`, and `Inf_f64` are reserved keywords. `Some`, `None`, `Ok`,
    ///   and `Err` remain ordinary identifiers.
    /// - Field names within an Object or struct variant and variant names within
    ///   an enumeration must be unique.
    /// - Tuples and Objects must be nonempty; tuple-like variants need at least
    ///   two elements and struct-like variants need at least one field.
    /// - An enumeration named `Option` must have exactly unit `None` and newtype
    ///   `Some(T)` variants. One named `Result` must have exactly newtype `Ok(T)`
    ///   and `Err(E)` variants. Their payload types and variant order may vary.
    ///
    /// A reference-table index that cannot fit a `u32` also returns an error.
    ///
    /// # Examples
    ///
    /// ```
    /// let schema = asonb::DocumentSchema::new(asonb::Type::Reference(0));
    /// assert!(schema.validate().is_err()); // The reference table is empty.
    /// ```
    pub fn validate(&self) -> Result<(), AsonbError> {
        let mut active = vec![false; self.references.len()];
        let mut visited = vec![false; self.references.len()];
        for index in 0..self.references.len() {
            let index = u32::try_from(index)
                .map_err(|_| AsonbError::Message("Too many schema definitions.".into()))?;
            self.validate_type(&Type::Reference(index), &mut active, &mut visited)?;
        }
        self.validate_type(&self.root, &mut active, &mut visited)?;
        Ok(())
    }

    /// Walks the type graph with separate "on this path" and "already done" marks.
    fn validate_type(
        &self,
        ty: &Type,
        active: &mut [bool],
        visited: &mut [bool],
    ) -> Result<(), AsonbError> {
        let children: Vec<&Type> = match ty {
            Type::Reference(index) => {
                let index = *index as usize;
                let referenced = self
                    .references
                    .get(index)
                    .ok_or_else(|| AsonbError::Message("Reference index out of range.".into()))?;
                // Reusing a completed definition is fine. Reaching a definition
                // still on this DFS path is a cycle, not harmless schema sharing.
                if active[index] {
                    return Err(AsonbError::Message("Cyclic type reference.".into()));
                }
                if !visited[index] {
                    active[index] = true;
                    self.validate_type(referenced, active, visited)?;
                    active[index] = false;
                    visited[index] = true;
                }
                return Ok(());
            }
            Type::List(_, item) => vec![item],
            Type::Map(_, key, value) => vec![key, value],
            Type::Tuple(items) => {
                if items.is_empty() {
                    return Err(AsonbError::Message(
                        "Tuple requires at least one element.".into(),
                    ));
                }
                items.iter().collect()
            }
            Type::Object(fields) => {
                validate_fields(fields)?;
                fields.iter().map(|field| &field.ty).collect()
            }
            Type::Enum(name, variants) => {
                if !is_valid_ason_identifier(name) {
                    return Err(AsonbError::Message(
                        "Enumeration name must be a valid ASON identifier.".into(),
                    ));
                }
                if RESERVED_KEYWORDS.contains(&name.as_str()) {
                    return Err(AsonbError::Message(format!(
                        "Enumeration name \"{name}\" is reserved by ASON."
                    )));
                }
                let mut names = HashSet::new();
                let mut children = Vec::new();
                for variant in variants {
                    if !is_valid_ason_identifier(variant.name()) {
                        return Err(AsonbError::Message(
                            "Variant name must be a valid ASON identifier.".into(),
                        ));
                    }
                    if RESERVED_KEYWORDS.contains(&variant.name()) {
                        return Err(AsonbError::Message(format!(
                            "Variant name \"{}\" is reserved by ASON.",
                            variant.name()
                        )));
                    }
                    if !names.insert(variant.name()) {
                        return Err(AsonbError::Message("Duplicate variant name.".into()));
                    }
                    match variant {
                        Variant::Unit(_) => {}
                        Variant::Newtype(_, ty) => children.push(ty),
                        Variant::Tuple(_, types) => {
                            if types.len() < 2 {
                                return Err(AsonbError::Message(
                                    "Tuple-like variant requires at least two elements.".into(),
                                ));
                            }
                            children.extend(types);
                        }
                        Variant::Struct(_, fields) => {
                            validate_fields(fields)?;
                            children.extend(fields.iter().map(|field| &field.ty));
                        }
                    }
                }
                validate_predefined_enumeration(name, variants)?;
                children
            }
            _ => vec![],
        };
        for child in children {
            self.validate_type(child, active, visited)?;
        }
        Ok(())
    }

    /// Follows reference aliases until a concrete type is found.
    /// The bounded walk also protects callers from an unvalidated cyclic table.
    pub(crate) fn resolve(&self, mut ty: Type) -> Result<Type, AsonbError> {
        for _ in 0..=self.references.len() {
            match ty {
                Type::Reference(index) => {
                    ty = self
                        .references
                        .get(index as usize)
                        .cloned()
                        .ok_or_else(|| {
                            AsonbError::Message("Reference index out of range.".into())
                        })?
                }
                _ => return Ok(ty),
            }
        }
        Err(AsonbError::Message("Cyclic type reference.".into()))
    }

    /// Writes the reference table and root type, but not the document header.
    /// Callers validate first so malformed schemas cannot emit partial documents.
    pub(crate) fn write(&self, writer: &mut impl Write) -> Result<(), AsonbError> {
        write_u32(writer, self.references.len() as u64)?;
        for ty in &self.references {
            write_type(writer, ty)?;
        }
        write_type(writer, &self.root)
    }

    /// Reads the entire schema and validates even unreachable reference entries.
    pub(crate) fn read<'de>(input: &mut impl Input<'de>) -> Result<Self, AsonbError> {
        let count = input.u32()?;
        let mut references = Vec::new();
        for _ in 0..count {
            references.push(read_type(input)?);
        }
        let root = read_type(input)?;
        let schema = Self { references, root };
        schema.validate()?;
        Ok(schema)
    }
}

/// Requires a nonempty field list with valid, unique wire identifiers.
fn validate_fields(fields: &[Field]) -> Result<(), AsonbError> {
    if fields.is_empty() {
        return Err(AsonbError::Message(
            "Object requires at least one field.".into(),
        ));
    }
    let mut names = HashSet::new();
    for field in fields {
        if !is_valid_ason_identifier(&field.name) {
            return Err(AsonbError::Message(
                "Field name must be a valid ASON identifier.".into(),
            ));
        }
        if RESERVED_KEYWORDS.contains(&field.name.as_str()) {
            return Err(AsonbError::Message(format!(
                "Field name \"{}\" is reserved by ASON.",
                field.name
            )));
        }
        if !names.insert(&field.name) {
            return Err(AsonbError::Message("Duplicate field name.".into()));
        }
    }
    Ok(())
}

/// Identifier-shaped ASON keywords that cannot be Object keys, Enumeration
/// names, or variant names: the Boolean keywords and the unsigned special
/// floating-point literals. The variant shorthands `Some`, `None`, `Ok`, and
/// `Err` are ordinary identifiers.
const RESERVED_KEYWORDS: [&str; 8] = [
    "true", "false", "NaN", "NaN_f32", "NaN_f64", "Inf", "Inf_f32", "Inf_f64",
];

/// Enforces ASON's fixed definitions of the predefined `Option` and `Result`.
///
/// `Option` must contain exactly `None` and `Some(T)`; `Result` must contain
/// exactly `Ok(T)` and `Err(E)`. Payload types and variant order (and thus wire
/// indices) are free. Names are case-sensitive, so `option` is an ordinary enum.
fn validate_predefined_enumeration(name: &str, variants: &[Variant]) -> Result<(), AsonbError> {
    let message = match name {
        "Option" => "Enumeration \"Option\" must have exactly the variants None and Some(...).",
        "Result" => "Enumeration \"Result\" must have exactly the variants Ok(...) and Err(...).",
        _ => return Ok(()),
    };
    // Variant names are already known to be unique, so two matching variants
    // are exactly the required pair.
    let valid = variants.len() == 2
        && variants.iter().all(|variant| match (name, variant) {
            ("Option", Variant::Unit(variant)) => variant == "None",
            ("Option", Variant::Newtype(variant, _)) => variant == "Some",
            ("Result", Variant::Newtype(variant, _)) => variant == "Ok" || variant == "Err",
            _ => false,
        });
    if valid {
        Ok(())
    } else {
        Err(AsonbError::Message(message.into()))
    }
}

/// Applies ASON's identifier ranges, which deliberately differ from Rust's.
fn is_valid_ason_identifier(value: &str) -> bool {
    /// Recognizes first characters; later characters may also be ASCII digits.
    fn is_start(character: char) -> bool {
        character.is_ascii_alphabetic()
            || character == '_'
            || ('\u{00a0}'..='\u{d7ff}').contains(&character)
            || ('\u{e000}'..='\u{10ffff}').contains(&character)
    }

    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    is_start(first) && characters.all(|character| is_start(character) || character.is_ascii_digit())
}

/// Writes a UTF-8 byte length and name, without a trailing NUL byte.
fn write_name(writer: &mut impl Write, name: &str) -> Result<(), AsonbError> {
    write_u32(writer, name.len() as u64)?;
    write_all(writer, name.as_bytes())
}

/// Reads an owned UTF-8 name and reports invalid text at its payload offset.
fn read_name<'de>(input: &mut impl Input<'de>) -> Result<String, AsonbError> {
    let length = input.u32()?;
    let offset = input.offset();
    let bytes = input.payload(length as u64)?;
    String::from_utf8(bytes.into_owned())
        .map_err(|error| AsonbError::MessageWithOffset(error.to_string(), offset))
}

/// Writes a field count followed by ordered name/type pairs.
fn write_fields(writer: &mut impl Write, fields: &[Field]) -> Result<(), AsonbError> {
    write_u32(writer, fields.len() as u64)?;
    for field in fields {
        write_name(writer, &field.name)?;
        write_type(writer, &field.ty)?;
    }
    Ok(())
}

/// Writes the count and ordered type descriptions for a tuple payload.
fn write_types(writer: &mut impl Write, types: &[Type]) -> Result<(), AsonbError> {
    write_u32(writer, types.len() as u64)?;
    for ty in types {
        write_type(writer, ty)?;
    }
    Ok(())
}

/// Emits a type-class tag, then only the metadata that this class requires.
fn write_type(writer: &mut impl Write, ty: &Type) -> Result<(), AsonbError> {
    let tag = match ty {
        Type::I8 => 0x13,
        Type::I16 => 0x14,
        Type::I32 => 0x15,
        Type::I64 => 0x16,
        Type::U8 => 0x23,
        Type::U16 => 0x24,
        Type::U32 => 0x25,
        Type::U64 => 0x26,
        Type::F32 => 0x35,
        Type::F64 => 0x36,
        Type::Bool => 0x40,
        Type::Char => 0x41,
        Type::DateTime => 0x42,
        Type::String(encoding) | Type::Bytes(encoding) | Type::List(encoding, _) => {
            let base = match ty {
                Type::String(_) => 0x50,
                Type::Bytes(_) => 0x60,
                _ => 0x70,
            };
            // The high nibble selects the container family; the low nibble
            // selects framing. Long form is 8, not the next sequential value.
            base + match encoding {
                Encoding::Regular => 0,
                Encoding::MultiChunk => 1,
                Encoding::Long => 8,
            }
        }
        Type::Map(long, _, _) => {
            if *long {
                0x88
            } else {
                0x80
            }
        }
        Type::Tuple(_) => 0x90,
        Type::Object(_) => 0xa0,
        Type::Enum(_, _) => 0xb0,
        Type::Reference(_) => 0xc0,
    };
    write_all(writer, &[tag])?;
    match ty {
        Type::String(_) | Type::Bytes(_) | Type::List(_, _) => {
            if let Type::List(_, item) = ty {
                write_type(writer, item)?;
            }
        }
        Type::Map(_, key, value) => {
            write_type(writer, key)?;
            write_type(writer, value)?;
        }
        Type::Tuple(types) => write_types(writer, types)?,
        Type::Object(fields) => write_fields(writer, fields)?,
        Type::Enum(name, variants) => {
            write_name(writer, name)?;
            write_u32(writer, variants.len() as u64)?;
            for variant in variants {
                let tag = match variant {
                    Variant::Unit(_) => 0,
                    Variant::Newtype(_, _) => 1,
                    Variant::Tuple(_, _) => 2,
                    Variant::Struct(_, _) => 4,
                };
                write_all(writer, &[tag])?;
                write_name(writer, variant.name())?;
                match variant {
                    Variant::Newtype(_, ty) => write_type(writer, ty)?,
                    Variant::Tuple(_, types) => write_types(writer, types)?,
                    Variant::Struct(_, fields) => write_fields(writer, fields)?,
                    _ => {}
                }
            }
        }
        Type::Reference(index) => write_u32(writer, *index as u64)?,
        _ => {}
    }
    Ok(())
}

/// Reads a counted list of type descriptions without guessing from value bytes.
fn read_types<'de>(input: &mut impl Input<'de>) -> Result<Vec<Type>, AsonbError> {
    let count = input.u32()?;
    let mut types = Vec::new();
    for _ in 0..count {
        types.push(read_type(input)?);
    }
    Ok(types)
}

/// Reads ordered name/type pairs; semantic checks happen after schema parsing.
fn read_fields<'de>(input: &mut impl Input<'de>) -> Result<Vec<Field>, AsonbError> {
    let count = input.u32()?;
    let mut fields = Vec::new();
    for _ in 0..count {
        fields.push(Field::new(read_name(input)?, read_type(input)?));
    }
    Ok(fields)
}

/// Parses one recursive type description and rejects unsupported type-class tags.
fn read_type<'de>(input: &mut impl Input<'de>) -> Result<Type, AsonbError> {
    let offset = input.offset();
    let tag = input.exact::<1>()?[0];
    Ok(match tag {
        0x13 => Type::I8,
        0x14 => Type::I16,
        0x15 => Type::I32,
        0x16 => Type::I64,
        0x23 => Type::U8,
        0x24 => Type::U16,
        0x25 => Type::U32,
        0x26 => Type::U64,
        0x35 => Type::F32,
        0x36 => Type::F64,
        0x40 => Type::Bool,
        0x41 => Type::Char,
        0x42 => Type::DateTime,
        0x50..=0x78 if matches!(tag & 15, 0 | 1 | 8) => {
            let encoding = match tag & 15 {
                0 => Encoding::Regular,
                1 => Encoding::MultiChunk,
                8 => Encoding::Long,
                _ => unreachable!(),
            };
            match tag >> 4 {
                5 => Type::String(encoding),
                6 => Type::Bytes(encoding),
                7 => Type::List(encoding, Box::new(read_type(input)?)),
                _ => unreachable!(),
            }
        }
        0x80 | 0x88 => Type::Map(
            tag == 0x88,
            Box::new(read_type(input)?),
            Box::new(read_type(input)?),
        ),
        0x90 => Type::Tuple(read_types(input)?),
        0xa0 => Type::Object(read_fields(input)?),
        0xb0 => {
            let name = read_name(input)?;
            let count = input.u32()?;
            let mut variants = Vec::new();
            for _ in 0..count {
                let form = input.exact::<1>()?[0];
                let name = read_name(input)?;
                variants.push(match form {
                    0 => Variant::Unit(name),
                    1 => Variant::Newtype(name, read_type(input)?),
                    2 => Variant::Tuple(name, read_types(input)?),
                    4 => Variant::Struct(name, read_fields(input)?),
                    _ => {
                        return Err(AsonbError::MessageWithOffset(
                            "Unknown variant form.".into(),
                            input.offset(),
                        ));
                    }
                });
            }
            Type::Enum(name, variants)
        }
        0xc0 => Type::Reference(input.u32()?),
        _ => {
            return Err(AsonbError::MessageWithOffset(
                format!("Unknown type class 0x{tag:02x}."),
                offset,
            ));
        }
    })
}

/// Supplies a static schema without inspecting a serialized value.
///
/// Usually implemented with `#[derive(asonb::Schema)]` alongside Serde derives.
/// Implementations must describe the actual Serde representation, not merely
/// the fields' Rust types. Use the supplied builder for child types so reference
/// reuse and recursive-type detection remain effective.
///
/// # Built-in Mappings
///
/// | Rust type | Wire representation |
/// | --- | --- |
/// | Fixed-width integers, `f32`, `f64`, `bool`, `char` | The corresponding scalar class |
/// | `isize`, `usize` | `I64`, `U64`, regardless of platform pointer width |
/// | `str`, `String` | Regular String |
/// | `serde_bytes::Bytes`, `serde_bytes::ByteBuf` | Regular Byte Data |
/// | `[T]`, `Vec<T>` | Regular List of `T` |
/// | `[T; N]` | A Tuple with `N` positions, unless the array field adapter is used |
/// | Rust tuples of 1 through 16 elements | Positional Tuple |
/// | `HashMap<K, V, S>`, `BTreeMap<K, V>` | Regular Map |
/// | `Option<T>`, `Result<T, E>` | The predefined enumeration, including every branch |
/// | `&T`, `Box<T>`, `Cow<'_, T>` | The wrapped type's representation |
/// | [`crate::date_time::DateTime`] | Binary DateTime |
///
/// A trait implementation alone does not guarantee a valid document schema.
/// For example, `()` and `[T; 0]` produce empty Tuples, which
/// [`DocumentSchema::of`] rejects. Unit enum variants and empty Lists are valid.
///
/// # Custom Representations
///
/// The schema must be independent of any particular value, including collection
/// contents and the selected enumeration branch. For derived named structs and
/// enums, recognized Serde adapters select their wire schemas automatically;
/// other field adapters require `#[asonb(schema_with = "path")]`. The function
/// at `path` takes `&mut SchemaBuilder` and returns `Result<Type, AsonbError>`.
/// See the [`derive@crate::Schema`] macro for supported attributes and forms.
///
/// ```
/// fn flag_schema(_: &mut asonb::SchemaBuilder)
///     -> Result<asonb::Type, asonb::AsonbError>
/// {
///     Ok(asonb::Type::U8)
/// }
///
/// fn serialize_flag<S: serde::Serializer>(value: &bool, serializer: S)
///     -> Result<S::Ok, S::Error>
/// {
///     serializer.serialize_u8(u8::from(*value))
/// }
///
/// #[derive(serde::Serialize, asonb::Schema)]
/// struct Flags {
///     #[serde(serialize_with = "serialize_flag")]
///     #[asonb(schema_with = "flag_schema")]
///     enabled: bool,
/// }
///
/// let bytes = asonb::ser_to_bytes(&Flags { enabled: true })?;
/// assert_eq!(
///     asonb::de_value_from_slice(&bytes)?.1,
///     asonb::Value::Object(vec![asonb::Value::U8(1)]),
/// );
/// # Ok::<(), asonb::AsonbError>(())
/// ```
///
/// Implementing this trait does not implement `Serialize` or `Deserialize`,
/// nor can it make an unsupported Serde operation valid.
pub trait Schema {
    /// Describes this type using `builder` to obtain any child schemas.
    ///
    /// `builder` is shared with the enclosing document; use
    /// [`SchemaBuilder::build`] for every child Rust type. The returned type can
    /// contain references created by that builder and is not necessarily a
    /// standalone schema. Do not retain it for use with another builder.
    ///
    /// Call [`SchemaBuilder::build`] rather than this method directly when
    /// assembling a document: `build` adds caching and recursion checks.
    ///
    /// # Errors
    ///
    /// Implementations should return an error when the representation cannot
    /// be described by ASONB and propagate failures from child schemas.
    /// [`DocumentSchema::of`] additionally validates the completed description.
    ///
    /// # Examples
    ///
    /// ```
    /// struct UserId(u64);
    /// impl asonb::Schema for UserId {
    ///     fn schema(builder: &mut asonb::SchemaBuilder)
    ///         -> Result<asonb::Type, asonb::AsonbError>
    ///     {
    ///         builder.build::<u64>()
    ///     }
    /// }
    /// let schema = asonb::DocumentSchema::of::<UserId>()?;
    /// assert_eq!(schema.root, asonb::Type::U64);
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError>;
}

/// Shared state for constructing a value-independent document schema.
///
/// [`Default::default`] creates an empty builder. [`Self::build`] caches results
/// by Rust type and deduplicates structurally identical Object, Tuple, and Enum
/// definitions into a private reference table, in first-completed order.
/// Re-entering a Rust type that is still being expanded is rejected as recursion.
///
/// This type is primarily for [`Schema`] implementations and field schema
/// overrides. Its returned references belong to this builder, and the reference
/// table is not publicly extractable. Use [`DocumentSchema::of`] when you need
/// a complete schema suitable for document I/O.
#[derive(Default)]
pub struct SchemaBuilder {
    /// Compound definitions in first-completed order, used by Reference indices.
    references: Vec<Type>,
    /// Rust type names currently being expanded, used to reject recursive types.
    active: Vec<&'static str>,
    /// Completed Rust types and their reusable schema or reference.
    cache: HashMap<&'static str, Type>,
}

impl SchemaBuilder {
    /// Builds the schema of `T`, reusing an earlier result when possible.
    ///
    /// `T` is selected through the type parameter; no runtime value is passed.
    /// Compound types are placed in this builder's private reference table, so
    /// a returned [`Type::Reference`] is meaningful only with that table. Use
    /// [`DocumentSchema::of`] when you need a standalone, complete schema.
    /// This method does not perform [`DocumentSchema::validate`]; validation
    /// happens after the complete document schema has been assembled.
    ///
    /// # Errors
    ///
    /// Returns an error for recursive expansion of `T`, errors from `T`'s
    /// [`Schema`] implementation or its children, or too many reusable
    /// definitions to index with a `u32`.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut builder = asonb::SchemaBuilder::default();
    /// assert_eq!(builder.build::<i32>()?, asonb::Type::I32);
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn build<T: Schema + ?Sized>(&mut self) -> Result<Type, AsonbError> {
        let name = std::any::type_name::<T>();
        if let Some(ty) = self.cache.get(name) {
            return Ok(ty.clone());
        }
        if self.active.contains(&name) {
            return Err(AsonbError::Message(format!("Recursive schema for {name}.")));
        }
        self.active.push(name);
        let result = T::schema(self);
        // Always remove the active mark, including when a custom schema fails.
        self.active.pop();
        let mut ty = result?;
        // Cache by Rust identity for speed, then deduplicate compound definitions
        // by structure so distinct Rust types can still share wire metadata.
        if matches!(ty, Type::Object(..) | Type::Tuple(..) | Type::Enum(..)) {
            let index = if let Some(index) = self.references.iter().position(|entry| entry == &ty) {
                index
            } else {
                self.references.push(ty);
                self.references.len() - 1
            };
            ty = Type::Reference(
                u32::try_from(index)
                    .map_err(|_| AsonbError::Message("Too many schema definitions.".into()))?,
            );
        }
        self.cache.insert(name, ty.clone());
        Ok(ty)
    }
}

/// Implements scalar mappings without allocating reference-table entries.
macro_rules! primitive_schema {
    ($($rust:ty => $ty:expr),* $(,)?)=>{$(impl Schema for $rust {
        /// Returns this Rust type's fixed wire representation.
        fn schema(_: &mut SchemaBuilder)->Result<Type,AsonbError>{Ok($ty)}
    })*};
}
primitive_schema!(i8=>Type::I8,i16=>Type::I16,i32=>Type::I32,i64=>Type::I64,isize=>Type::I64,u8=>Type::U8,u16=>Type::U16,u32=>Type::U32,u64=>Type::U64,usize=>Type::U64,f32=>Type::F32,f64=>Type::F64,bool=>Type::Bool,char=>Type::Char,str=>Type::String(Encoding::Regular),String=>Type::String(Encoding::Regular),()=>Type::Tuple(vec![]));
impl<T: Schema + ToOwned + ?Sized> Schema for std::borrow::Cow<'_, T> {
    /// Ownership does not change the borrowed type's wire representation.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        builder.build::<T>()
    }
}
primitive_schema!(serde_bytes::Bytes=>Type::Bytes(Encoding::Regular),serde_bytes::ByteBuf=>Type::Bytes(Encoding::Regular));
impl<T: Schema + ?Sized> Schema for &T {
    /// References are transparent values, not pointer-identity records.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        builder.build::<T>()
    }
}
impl<T: Schema + ?Sized> Schema for Box<T> {
    /// Heap allocation does not add a wire-level wrapper.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        builder.build::<T>()
    }
}
impl<T: Schema> Schema for [T] {
    /// Describes homogeneous elements even when a particular slice is empty.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::List(
            Encoding::Regular,
            Box::new(builder.build::<T>()?),
        ))
    }
}
impl<T: Schema> Schema for Vec<T> {
    /// Uses the same homogeneous List mapping as a slice.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        <[T]>::schema(builder)
    }
}
impl<T: Schema, const SIZE: usize> Schema for [T; SIZE] {
    /// Matches Serde's default tuple mapping; use the array adapter for a List.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::Tuple(vec![builder.build::<T>()?; SIZE]))
    }
}
impl<T: Schema> Schema for Option<T> {
    /// Describes both None and Some, regardless of the current value.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::Enum(
            "Option".into(),
            vec![
                Variant::Unit("None".into()),
                Variant::Newtype("Some".into(), builder.build::<T>()?),
            ],
        ))
    }
}
impl<T: Schema, E: Schema> Schema for Result<T, E> {
    /// Describes both result branches, each with a single typed payload.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::Enum(
            "Result".into(),
            vec![
                Variant::Newtype("Ok".into(), builder.build::<T>()?),
                Variant::Newtype("Err".into(), builder.build::<E>()?),
            ],
        ))
    }
}
impl<K: Schema + Eq + Hash, V: Schema, S: std::hash::BuildHasher> Schema for HashMap<K, V, S> {
    /// Records key and value types, not hash iteration order or hasher state.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::Map(
            false,
            Box::new(builder.build::<K>()?),
            Box::new(builder.build::<V>()?),
        ))
    }
}
impl<K: Schema + Ord, V: Schema> Schema for BTreeMap<K, V> {
    /// Uses regular Map framing; ordering comes from the serialized value.
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::Map(
            false,
            Box::new(builder.build::<K>()?),
            Box::new(builder.build::<V>()?),
        ))
    }
}
/// Generates positional tuple schemas for the supported Rust tuple arities.
macro_rules! tuple_schema {
    ($($name:ident),+)=>{impl<$($name:Schema),+> Schema for ($($name,)+){
        /// Builds each position independently, preserving Rust tuple order.
        fn schema(builder:&mut SchemaBuilder)->Result<Type,AsonbError>{Ok(Type::Tuple(vec![$(builder.build::<$name>()?),+]))}
    }};
}
tuple_schema!(A);
tuple_schema!(A, B);
tuple_schema!(A, B, C);
tuple_schema!(A, B, C, D);
tuple_schema!(A, B, C, D, E);
tuple_schema!(A, B, C, D, E, F);
tuple_schema!(A, B, C, D, E, F, G);
tuple_schema!(A, B, C, D, E, F, G, H);
tuple_schema!(A, B, C, D, E, F, G, H, I);
tuple_schema!(A, B, C, D, E, F, G, H, I, J);
tuple_schema!(A, B, C, D, E, F, G, H, I, J, K);
tuple_schema!(A, B, C, D, E, F, G, H, I, J, K, L);
tuple_schema!(A, B, C, D, E, F, G, H, I, J, K, L, M);
tuple_schema!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
tuple_schema!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
tuple_schema!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);

/// Turns a variant shape into the type used by the shared payload machinery.
/// The empty unit Tuple is internal: it is not a standalone document schema.
pub(crate) fn payload_type(variant: &Variant) -> Type {
    match variant {
        Variant::Unit(_) => Type::Tuple(vec![]),
        Variant::Newtype(_, ty) => ty.clone(),
        Variant::Tuple(_, types) => Type::Tuple(types.clone()),
        Variant::Struct(_, fields) => Type::Object(fields.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::SliceInput;

    #[test]
    /// Round-trips every supported type description and rejects all truncations.
    fn test_all_type_definitions() {
        let mut types = vec![
            Type::I8,
            Type::I16,
            Type::I32,
            Type::I64,
            Type::U8,
            Type::U16,
            Type::U32,
            Type::U64,
            Type::F32,
            Type::F64,
            Type::Bool,
            Type::Char,
            Type::DateTime,
            Type::Tuple(vec![Type::Bool]),
            Type::Object(vec![Field::new("a", Type::I32)]),
            Type::Enum(
                "E".into(),
                vec![
                    Variant::Unit("A".into()),
                    Variant::Newtype("B".into(), Type::I32),
                    Variant::Tuple("C".into(), vec![Type::Bool, Type::I32]),
                    Variant::Struct("D".into(), vec![Field::new("x", Type::Char)]),
                ],
            ),
        ];
        for encoding in [Encoding::Regular, Encoding::Long, Encoding::MultiChunk] {
            types.push(Type::String(encoding.clone()));
            types.push(Type::Bytes(encoding.clone()));
            types.push(Type::List(encoding, Box::new(Type::I8)));
        }
        types.push(Type::Map(false, Box::new(Type::U8), Box::new(Type::I32)));
        types.push(Type::Map(true, Box::new(Type::U8), Box::new(Type::I32)));
        for ty in types {
            let schema = DocumentSchema::new(ty);
            let mut bytes = Vec::new();
            schema.write(&mut bytes).unwrap();
            let mut input = SliceInput::new(&bytes);
            assert_eq!(DocumentSchema::read(&mut input).unwrap(), schema);
            input.finish().unwrap();
            for end in 0..bytes.len() {
                assert!(DocumentSchema::read(&mut SliceInput::new(&bytes[..end])).is_err());
            }
        }
    }

    #[test]
    /// Keeps reserved container-framing tags from being accepted accidentally.
    fn test_unsupported_type_classes_are_rejected() {
        for tag in [0x52, 0x59, 0x62, 0x69, 0x72, 0x79] {
            assert!(
                read_type(&mut SliceInput::new(&[tag])).is_err(),
                "tag {tag:#04x}"
            );
        }
    }

    #[test]
    /// Locks down the exact MultiChunk type tags for text, byte data, and Lists.
    fn test_multi_chunk_type_classes() {
        for (ty, expected_tag) in [
            (Type::String(Encoding::MultiChunk), 0x51),
            (Type::Bytes(Encoding::MultiChunk), 0x61),
            (Type::List(Encoding::MultiChunk, Box::new(Type::I32)), 0x71),
        ] {
            let schema = DocumentSchema::new(ty);
            let mut bytes = Vec::new();
            schema.write(&mut bytes).unwrap();
            assert_eq!(bytes[4], expected_tag);
            assert_eq!(
                DocumentSchema::read(&mut SliceInput::new(&bytes)).unwrap(),
                schema
            );
        }
    }

    #[test]
    /// Requires every I/O entry point to reject invalid arities before payloads.
    fn test_container_arity_validation() {
        for ty in [
            Type::Tuple(vec![]),
            Type::Object(vec![]),
            Type::Enum("E".into(), vec![Variant::Tuple("Empty".into(), vec![])]),
            Type::Enum(
                "E".into(),
                vec![Variant::Tuple("One".into(), vec![Type::I32])],
            ),
            Type::Enum("E".into(), vec![Variant::Struct("Empty".into(), vec![])]),
        ] {
            let schema = DocumentSchema::new(ty.clone());
            let expected = schema.validate().unwrap_err().to_string();
            let mut output = Vec::new();
            assert_eq!(
                crate::ser_value_to_writer(&schema, &crate::Value::I32(1), &mut output)
                    .unwrap_err()
                    .to_string(),
                expected
            );
            assert!(output.is_empty());
            let mut bytes = crate::binary::HEADER.to_vec();
            schema.write(&mut bytes).unwrap();
            for error in [
                crate::de_from_slice::<i32>(&bytes).unwrap_err(),
                crate::de_from_reader::<i32, _>(bytes.as_slice()).unwrap_err(),
                crate::de_value_from_slice(&bytes).unwrap_err(),
                crate::de_value_from_reader(bytes.as_slice()).unwrap_err(),
            ] {
                assert_eq!(error.to_string(), expected);
            }

            let schema = DocumentSchema {
                references: vec![ty],
                root: Type::List(Encoding::MultiChunk, Box::new(Type::I32)),
            };
            let mut bytes = crate::binary::HEADER.to_vec();
            schema.write(&mut bytes).unwrap();
            assert_eq!(
                crate::list_stream_reader::<i32, _>(bytes.as_slice())
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
        }
    }

    #[test]
    /// Checks forward aliases, invalid indices, cycles, and duplicate wire names.
    fn test_reference_and_name_validation() {
        let valid = DocumentSchema {
            references: vec![Type::Reference(1), Type::I32],
            root: Type::Reference(0),
        };
        valid.validate().unwrap();
        for refs in [
            vec![Type::Reference(0)],
            vec![Type::Reference(1), Type::Reference(0)],
            vec![Type::Reference(5)],
        ] {
            assert!(
                DocumentSchema {
                    references: refs,
                    root: Type::I32
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            DocumentSchema::new(Type::Object(vec![
                Field::new("x", Type::I8),
                Field::new("x", Type::I8)
            ]))
            .validate()
            .is_err()
        );
        assert!(
            DocumentSchema::new(Type::Enum(
                "E".into(),
                vec![Variant::Unit("x".into()), Variant::Unit("x".into())]
            ))
            .validate()
            .is_err()
        );
        let empty_enum =
            DocumentSchema::new(Type::Enum("".into(), vec![Variant::Unit("x".into())]));
        assert!(matches!(
            empty_enum.validate(),
            Err(AsonbError::Message(message)) if message == "Enumeration name must be a valid ASON identifier."
        ));
        let mut output = Vec::new();
        assert!(
            crate::ser_value_to_writer(&empty_enum, &crate::Value::I32(1), &mut output).is_err()
        );
        assert!(output.is_empty());
        let mut schema_bytes = Vec::new();
        empty_enum.write(&mut schema_bytes).unwrap();
        assert!(DocumentSchema::read(&mut SliceInput::new(&schema_bytes)).is_err());
        assert!(DocumentSchema::read(&mut SliceInput::new(&[0, 0, 0, 0, 0xff])).is_err());
    }

    #[test]
    /// Covers ASON's Unicode ranges and disallowed identifier punctuation.
    fn test_ason_identifier_validation() {
        for name in ["_field2", "field_2", "文🙂"] {
            DocumentSchema::new(Type::Object(vec![Field::new(name, Type::I8)]))
                .validate()
                .unwrap();
            DocumentSchema::new(Type::Enum(name.into(), vec![Variant::Unit(name.into())]))
                .validate()
                .unwrap();
        }

        for name in [
            "",
            "2field",
            "has space",
            "has-hyphen",
            "has::separator",
            "\u{7f}",
        ] {
            assert!(
                DocumentSchema::new(Type::Object(vec![Field::new(name, Type::I8)],))
                    .validate()
                    .is_err()
            );
            assert!(
                DocumentSchema::new(Type::Enum(name.into(), vec![Variant::Unit("Valid".into())],))
                    .validate()
                    .is_err()
            );
            assert!(
                DocumentSchema::new(Type::Enum("Valid".into(), vec![Variant::Unit(name.into())],))
                    .validate()
                    .is_err()
            );
        }
    }

    #[test]
    /// Rejects cycles even in unused entries while allowing completed sharing.
    fn test_cyclic_document_rejected_at_io_boundaries() {
        /// Requires the same cycle diagnostic through each reading/writing API.
        fn assert_cycle(error: AsonbError) {
            assert!(
                matches!(error, AsonbError::Message(message) if message == "Cyclic type reference.")
            );
        }
        for references in [
            vec![Type::Reference(0)],
            vec![Type::Reference(1), Type::Reference(0)],
            vec![Type::List(Encoding::Regular, Box::new(Type::Reference(0)))],
            vec![Type::Object(vec![Field::new("next", Type::Reference(0))])],
            vec![Type::Enum(
                "Node".into(),
                vec![Variant::Newtype("Next".into(), Type::Reference(0))],
            )],
        ] {
            let mut schema = DocumentSchema {
                references,
                root: Type::I32,
            };
            let mut output = Vec::new();
            assert_cycle(
                crate::ser_value_to_writer(&schema, &crate::Value::I32(11), &mut output)
                    .unwrap_err(),
            );
            assert!(output.is_empty());
            let mut bytes = crate::binary::HEADER.to_vec();
            schema.write(&mut bytes).unwrap();
            assert_cycle(crate::de_from_slice::<i32>(&bytes).unwrap_err());
            assert_cycle(crate::de_from_reader::<i32, _>(bytes.as_slice()).unwrap_err());
            assert_cycle(crate::de_value_from_slice(&bytes).unwrap_err());
            assert_cycle(crate::de_value_from_reader(bytes.as_slice()).unwrap_err());
            schema.root = Type::List(Encoding::MultiChunk, Box::new(Type::I32));
            let mut bytes = crate::binary::HEADER.to_vec();
            schema.write(&mut bytes).unwrap();
            assert_cycle(
                crate::list_stream_reader::<i32, _>(bytes.as_slice())
                    .err()
                    .unwrap(),
            );
        }
        let shared = DocumentSchema {
            references: vec![
                Type::Tuple(vec![Type::Reference(1), Type::Reference(1)]),
                Type::I32,
            ],
            root: Type::Reference(0),
        };
        shared.validate().unwrap();
    }

    #[test]
    /// Fixes the variant names and payload forms of `Option` and `Result`.
    fn test_predefined_enumerations() {
        let none = || Variant::Unit("None".into());
        let some = |ty| Variant::Newtype("Some".into(), ty);
        let ok = |ty| Variant::Newtype("Ok".into(), ty);
        let err = |ty| Variant::Newtype("Err".into(), ty);

        // Any payload types and either variant order are allowed.
        for (name, variants) in [
            ("Option", vec![none(), some(Type::I32)]),
            ("Option", vec![some(Type::String(Encoding::Long)), none()]),
            (
                "Result",
                vec![ok(Type::I32), err(Type::String(Encoding::Regular))],
            ),
            (
                "Result",
                vec![err(Type::Bool), ok(Type::Tuple(vec![Type::I8]))],
            ),
            // Names are case-sensitive, so these are ordinary user enumerations.
            ("option", vec![Variant::Unit("Other".into())]),
            ("RESULT", vec![ok(Type::I32)]),
        ] {
            DocumentSchema::new(Type::Enum(name.into(), variants))
                .validate()
                .unwrap();
        }
        DocumentSchema::of::<Option<Result<Vec<i32>, String>>>().unwrap();

        for (name, variants) in [
            ("Option", vec![]),
            ("Option", vec![none()]),
            ("Option", vec![some(Type::I32)]),
            (
                "Option",
                vec![none(), some(Type::I32), Variant::Unit("Other".into())],
            ),
            (
                "Option",
                vec![Variant::Newtype("None".into(), Type::I32), some(Type::I32)],
            ),
            ("Option", vec![none(), Variant::Unit("Some".into())]),
            (
                "Option",
                vec![
                    none(),
                    Variant::Tuple("Some".into(), vec![Type::I32, Type::I32]),
                ],
            ),
            (
                "Option",
                vec![
                    none(),
                    Variant::Struct("Some".into(), vec![Field::new("x", Type::I32)]),
                ],
            ),
            (
                "Option",
                vec![Variant::Unit("none".into()), some(Type::I32)],
            ),
            (
                "Option",
                vec![none(), Variant::Newtype("SOME".into(), Type::I32)],
            ),
            ("Result", vec![ok(Type::I32)]),
            ("Result", vec![err(Type::I32)]),
            (
                "Result",
                vec![ok(Type::I32), err(Type::I32), Variant::Unit("Other".into())],
            ),
            ("Result", vec![Variant::Unit("Ok".into()), err(Type::I32)]),
            ("Result", vec![ok(Type::I32), Variant::Unit("Err".into())]),
            (
                "Result",
                vec![
                    ok(Type::I32),
                    Variant::Tuple("Err".into(), vec![Type::I32, Type::I32]),
                ],
            ),
            (
                "Result",
                vec![Variant::Newtype("ok".into(), Type::I32), err(Type::I32)],
            ),
            ("Result", vec![none(), some(Type::I32)]),
            ("Option", vec![ok(Type::I32), err(Type::I32)]),
        ] {
            let ty = Type::Enum(name.into(), variants);
            let expected = format!(
                "Enumeration \"{name}\" must have exactly the variants {}.",
                if name == "Option" {
                    "None and Some(...)"
                } else {
                    "Ok(...) and Err(...)"
                }
            );
            assert!(
                matches!(DocumentSchema::new(ty.clone()).validate(), Err(AsonbError::Message(message)) if message == expected),
                "accepted {ty:?}"
            );
            // Unused reference definitions are validated too, including on read.
            let schema = DocumentSchema {
                references: vec![ty],
                root: Type::I32,
            };
            let mut bytes = crate::binary::HEADER.to_vec();
            schema.write(&mut bytes).unwrap();
            bytes.extend_from_slice(&11i32.to_le_bytes());
            assert!(crate::de_from_slice::<i32>(&bytes).is_err());
        }
    }

    #[test]
    /// Rejects ASON keywords as Object keys, object-like variant keys,
    /// enumeration names, and variant names.
    fn test_reserved_object_keys() {
        for key in [
            "true", "false", "NaN", "NaN_f32", "NaN_f64", "Inf", "Inf_f32", "Inf_f64",
        ] {
            let expected = format!("Field name \"{key}\" is reserved by ASON.");
            for ty in [
                Type::Object(vec![Field::new(key, Type::I32)]),
                Type::Enum(
                    "E".into(),
                    vec![Variant::Struct(
                        "V".into(),
                        vec![Field::new(key, Type::I32)],
                    )],
                ),
            ] {
                assert!(
                    matches!(DocumentSchema::new(ty).validate(), Err(AsonbError::Message(message)) if message == expected),
                    "accepted reserved key {key}"
                );
            }

            let enumeration = Type::Enum(key.into(), vec![Variant::Unit("A".into())]);
            let expected = format!("Enumeration name \"{key}\" is reserved by ASON.");
            assert!(
                matches!(DocumentSchema::new(enumeration).validate(), Err(AsonbError::Message(message)) if message == expected),
                "accepted reserved enumeration name {key}"
            );

            let variant = Type::Enum("E".into(), vec![Variant::Unit(key.into())]);
            let expected = format!("Variant name \"{key}\" is reserved by ASON.");
            assert!(
                matches!(DocumentSchema::new(variant).validate(), Err(AsonbError::Message(message)) if message == expected),
                "accepted reserved variant name {key}"
            );
        }

        // Only exact spellings are reserved, and the variant shorthands are ordinary identifiers.
        for key in [
            "some", "NONE", "ok", "ERR", "True", "nan", "Inf_i32", "Some_", "Some", "None", "Ok",
            "Err",
        ] {
            DocumentSchema::new(Type::Object(vec![Field::new(key, Type::I32)]))
                .validate()
                .unwrap();
        }
        DocumentSchema::new(Type::Enum(
            "Some".into(),
            vec![Variant::Unit("None".into()), Variant::Unit("Ok".into())],
        ))
        .validate()
        .unwrap();
    }

    #[test]
    /// Checks reusable compound definitions and the absence of an arity quota.
    fn test_static_schemas() {
        let schema = DocumentSchema::of::<Vec<Option<(i32, String)>>>().unwrap();
        assert_eq!(schema.references.len(), 2);
        assert_eq!(
            DocumentSchema::of::<i32>().unwrap(),
            DocumentSchema::new(Type::I32)
        );
        let schema = DocumentSchema::of::<[i32; 1_000_001]>().unwrap();
        let Type::Tuple(types) = &schema.references[0] else {
            panic!("Expected a tuple schema.");
        };
        assert_eq!(types.len(), 1_000_001);
    }
}
