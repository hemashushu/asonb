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

/// Every type class defined by version 1 of ASONB.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Char,
    DateTime,
    String(Encoding),
    Bytes(Encoding),
    List(Encoding, Box<Type>),
    Map(
        /* is_long_regular_map */ bool,
        /* name type */ Box<Type>,
        /* value type */ Box<Type>,
    ),
    Tuple(Vec<Type>),
    Object(/* object name */ String, Vec<Field>),
    Enum(/* enum name */ String, Vec<Variant>),
    Reference(u32),
}

/// Forms of Strings, Byte Data, and List.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Encoding {
    Regular,
    Long,
    MultiChunk,
}

/// Object field description.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}

impl Field {
    pub fn new(name: impl Into<String>, ty: Type) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

/// The four enumeration payload forms. Their wire form tags are 0, 1, 2, 4.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Variant {
    /// A variant with no payload.
    Unit(String),

    /// A variant with a single payload.
    Newtype(String, Type),

    /// A variant with a tuple payload.
    Tuple(String, Vec<Type>),

    /// A variant with a struct payload.
    Struct(String, Vec<Field>),
}

impl Variant {
    pub fn name(&self) -> &str {
        match self {
            Self::Unit(name)
            | Self::Newtype(name, _)
            | Self::Tuple(name, _)
            | Self::Struct(name, _) => name,
        }
    }
}

/// A complete document schema. Reference indices are local to this table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSchema {
    /// Reference types used in this schema.
    pub references: Vec<Type>,

    /// The root value type of the document.
    pub root: Type,
}

impl DocumentSchema {
    pub fn new(root: Type) -> Self {
        Self {
            references: Vec::new(),
            root,
        }
    }

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

    /// Validate every entry, including unused definitions. Completed references
    /// are cached so shared schemas are checked only once.
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
            Type::Object(_, fields) => {
                validate_fields(fields)?;
                fields.iter().map(|field| &field.ty).collect()
            }
            Type::Enum(name, variants) => {
                if !is_valid_ason_identifier(name) {
                    return Err(AsonbError::Message(
                        "Enumeration name must be a valid ASON identifier.".into(),
                    ));
                }
                let mut names = HashSet::new();
                let mut children = Vec::new();
                for variant in variants {
                    if !is_valid_ason_identifier(variant.name()) {
                        return Err(AsonbError::Message(
                            "Variant name must be a valid ASON identifier.".into(),
                        ));
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
                children
            }
            _ => vec![],
        };
        for child in children {
            self.validate_type(child, active, visited)?;
        }
        Ok(())
    }

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

    pub(crate) fn write(&self, writer: &mut impl Write) -> Result<(), AsonbError> {
        write_u32(writer, self.references.len() as u64)?;
        for ty in &self.references {
            write_type(writer, ty)?;
        }
        write_type(writer, &self.root)
    }

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
        if !names.insert(&field.name) {
            return Err(AsonbError::Message("Duplicate field name.".into()));
        }
    }
    Ok(())
}

fn is_valid_ason_identifier(value: &str) -> bool {
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

fn write_name(writer: &mut impl Write, name: &str) -> Result<(), AsonbError> {
    write_u32(writer, name.len() as u64)?;
    write_all(writer, name.as_bytes())
}

fn read_name<'de>(input: &mut impl Input<'de>) -> Result<String, AsonbError> {
    let length = input.u32()?;
    let offset = input.offset();
    let bytes = input.payload(length as u64)?;
    String::from_utf8(bytes.into_owned())
        .map_err(|error| AsonbError::MessageWithOffset(error.to_string(), offset))
}

fn write_fields(writer: &mut impl Write, fields: &[Field]) -> Result<(), AsonbError> {
    write_u32(writer, fields.len() as u64)?;
    for field in fields {
        write_name(writer, &field.name)?;
        write_type(writer, &field.ty)?;
    }
    Ok(())
}

fn write_types(writer: &mut impl Write, types: &[Type]) -> Result<(), AsonbError> {
    write_u32(writer, types.len() as u64)?;
    for ty in types {
        write_type(writer, ty)?;
    }
    Ok(())
}

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
        Type::Object(_, _) => 0xa0,
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
        Type::Object(name, fields) => {
            write_name(writer, name)?;
            write_fields(writer, fields)?;
        }
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

fn read_types<'de>(input: &mut impl Input<'de>) -> Result<Vec<Type>, AsonbError> {
    let count = input.u32()?;
    let mut types = Vec::new();
    for _ in 0..count {
        types.push(read_type(input)?);
    }
    Ok(types)
}

fn read_fields<'de>(input: &mut impl Input<'de>) -> Result<Vec<Field>, AsonbError> {
    let count = input.u32()?;
    let mut fields = Vec::new();
    for _ in 0..count {
        fields.push(Field::new(read_name(input)?, read_type(input)?));
    }
    Ok(fields)
}

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
        0xa0 => Type::Object(read_name(input)?, read_fields(input)?),
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
pub trait Schema {
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError>;
}

/// Builds deterministic reusable type definitions and rejects Rust recursion.
#[derive(Default)]
pub struct SchemaBuilder {
    references: Vec<Type>,
    active: Vec<&'static str>,
    cache: HashMap<&'static str, Type>,
}

impl SchemaBuilder {
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
        self.active.pop();
        let mut ty = result?;
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

macro_rules! primitive_schema {
    ($($rust:ty => $ty:expr),* $(,)?)=>{$(impl Schema for $rust {fn schema(_: &mut SchemaBuilder)->Result<Type,AsonbError>{Ok($ty)}})*};
}
primitive_schema!(i8=>Type::I8,i16=>Type::I16,i32=>Type::I32,i64=>Type::I64,isize=>Type::I64,u8=>Type::U8,u16=>Type::U16,u32=>Type::U32,u64=>Type::U64,usize=>Type::U64,f32=>Type::F32,f64=>Type::F64,bool=>Type::Bool,char=>Type::Char,str=>Type::String(Encoding::Regular),String=>Type::String(Encoding::Regular),()=>Type::Tuple(vec![]));
impl<T: Schema + ToOwned + ?Sized> Schema for std::borrow::Cow<'_, T> {
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        builder.build::<T>()
    }
}
primitive_schema!(serde_bytes::Bytes=>Type::Bytes(Encoding::Regular),serde_bytes::ByteBuf=>Type::Bytes(Encoding::Regular));
impl<T: Schema + ?Sized> Schema for &T {
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        builder.build::<T>()
    }
}
impl<T: Schema + ?Sized> Schema for Box<T> {
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        builder.build::<T>()
    }
}
impl<T: Schema> Schema for [T] {
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::List(
            Encoding::Regular,
            Box::new(builder.build::<T>()?),
        ))
    }
}
impl<T: Schema> Schema for Vec<T> {
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        <[T]>::schema(builder)
    }
}
impl<T: Schema, const SIZE: usize> Schema for [T; SIZE] {
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::Tuple(vec![builder.build::<T>()?; SIZE]))
    }
}
impl<T: Schema> Schema for Option<T> {
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
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::Map(
            false,
            Box::new(builder.build::<K>()?),
            Box::new(builder.build::<V>()?),
        ))
    }
}
impl<K: Schema + Ord, V: Schema> Schema for BTreeMap<K, V> {
    fn schema(builder: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::Map(
            false,
            Box::new(builder.build::<K>()?),
            Box::new(builder.build::<V>()?),
        ))
    }
}
macro_rules! tuple_schema {
    ($($name:ident),+)=>{impl<$($name:Schema),+> Schema for ($($name,)+){fn schema(builder:&mut SchemaBuilder)->Result<Type,AsonbError>{Ok(Type::Tuple(vec![$(builder.build::<$name>()?),+]))}}};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binary::SliceInput;

    #[test]
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
            Type::Object("".into(), vec![Field::new("a", Type::I32)]),
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
    fn test_unsupported_type_classes_are_rejected() {
        for tag in [0x52, 0x59, 0x62, 0x69, 0x72, 0x79] {
            assert!(
                read_type(&mut SliceInput::new(&[tag])).is_err(),
                "tag {tag:#04x}"
            );
        }
    }

    #[test]
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
    fn test_container_arity_validation() {
        for ty in [
            Type::Tuple(vec![]),
            Type::Object("Empty".into(), vec![]),
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
            DocumentSchema::new(Type::Object(
                "".into(),
                vec![Field::new("x", Type::I8), Field::new("x", Type::I8)]
            ))
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
    fn test_ason_identifier_validation() {
        for name in ["_field2", "field_2", "文🙂"] {
            DocumentSchema::new(Type::Object("".into(), vec![Field::new(name, Type::I8)]))
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
                DocumentSchema::new(Type::Object("".into(), vec![Field::new(name, Type::I8)],))
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
    fn test_cyclic_document_rejected_at_io_boundaries() {
        fn assert_cycle(error: AsonbError) {
            assert!(
                matches!(error, AsonbError::Message(message) if message == "Cyclic type reference.")
            );
        }
        for references in [
            vec![Type::Reference(0)],
            vec![Type::Reference(1), Type::Reference(0)],
            vec![Type::List(Encoding::Regular, Box::new(Type::Reference(0)))],
            vec![Type::Object(
                "Node".into(),
                vec![Field::new("next", Type::Reference(0))],
            )],
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
