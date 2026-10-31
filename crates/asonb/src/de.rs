// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use crate::{
    AsonbError, DocumentSchema, Encoding, Field, Type, Value, Variant,
    binary::{Input, ReaderInput, SliceInput},
    value::{payload_type, read_blob, read_length, read_value, skip_value},
};
use serde::{
    Deserialize,
    de::{
        self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
        Visitor,
    },
};
use std::{borrow::Cow, io::Read, iter::FusedIterator, marker::PhantomData};

/// Read one finite document into an owned Rust value. This does not require
/// Schema on the target type: the writer's schema travels with the document.
pub fn de_from_reader<T: de::DeserializeOwned, R: Read>(reader: R) -> Result<T, AsonbError> {
    read_document(&mut ReaderInput::new(reader))
}

/// Borrow contiguous string/byte payloads from a slice. Multi-chunk payloads
/// require an owned target because their prefixes interrupt contiguity.
pub fn de_from_slice<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, AsonbError> {
    read_document(&mut SliceInput::new(bytes))
}

fn read_document<'de, T: Deserialize<'de>, I: Input<'de>>(input: &mut I) -> Result<T, AsonbError> {
    input.header()?;
    let schema = DocumentSchema::read(input)?;
    let value = T::deserialize(&mut Deserializer::new(
        input,
        &schema,
        schema.root.clone(),
        true,
    ))?;
    input.finish()?;
    Ok(value)
}

struct Deserializer<'a, 'de, I: Input<'de>> {
    input: &'a mut I,
    schema: &'a DocumentSchema,
    ty: Type,
    root: bool,
    marker: PhantomData<&'de ()>,
}

impl<'a, 'de, I: Input<'de>> Deserializer<'a, 'de, I> {
    fn new(input: &'a mut I, schema: &'a DocumentSchema, ty: Type, root: bool) -> Self {
        Self {
            input,
            schema,
            ty,
            root,
            marker: PhantomData,
        }
    }
    fn ty(&self) -> Result<Type, AsonbError> {
        self.schema.resolve(self.ty.clone())
    }
    fn mismatch(&self, expected: &str) -> AsonbError {
        AsonbError::MessageWithOffset(
            format!("Expected {expected}, found {:?}.", self.ty),
            self.input.offset(),
        )
    }
    fn value(&mut self) -> Result<Value<'de>, AsonbError> {
        read_value(self.input, self.schema, &self.ty, self.root)
    }
    fn enumeration(&mut self) -> Result<(u32, Variant), AsonbError> {
        let Type::Enum(_, variants) = self.ty()? else {
            return Err(self.mismatch("an Enumeration"));
        };
        let index = self.input.u32()?;
        let variant = variants.get(index as usize).cloned().ok_or_else(|| {
            AsonbError::MessageWithOffset(
                "Variant index out of range.".into(),
                self.input.offset() - 4,
            )
        })?;
        Ok((index, variant))
    }
    fn sequence<V: Visitor<'de>>(
        &mut self,
        visitor: V,
        length: Option<usize>,
    ) -> Result<V::Value, AsonbError> {
        let mode = match self.ty()? {
            Type::List(encoding, item) if length.is_none() => {
                let remaining = if encoding == Encoding::MultiChunk {
                    0
                } else {
                    read_length(self.input, &encoding)?
                };
                SequenceMode::List {
                    encoding,
                    item: *item,
                    remaining,
                    ended: false,
                }
            }
            Type::Tuple(types) if length == Some(types.len()) => SequenceMode::Tuple(types, 0),
            _ => return Err(self.mismatch("a matching List or Tuple")),
        };
        let mut access = SeqAccessor {
            deserializer: self,
            mode,
        };
        let value = visitor.visit_seq(&mut access)?;
        if !access.complete() {
            return Err(AsonbError::Message(
                "Visitor did not consume the complete sequence.".into(),
            ));
        }
        Ok(value)
    }
    fn mapping<V: Visitor<'de>>(
        &mut self,
        visitor: V,
        object: bool,
    ) -> Result<V::Value, AsonbError> {
        let mode = match self.ty()? {
            Type::Object(_, fields) if object => MappingMode::Object(fields),
            Type::Map(long, key, value) if !object => {
                let count = read_length(
                    self.input,
                    &if long {
                        Encoding::Long
                    } else {
                        Encoding::Regular
                    },
                )?;
                MappingMode::Map {
                    key: *key,
                    value: *value,
                    count,
                }
            }
            _ => return Err(self.mismatch("a matching Object or Map")),
        };
        let mut access = MapAccessor {
            deserializer: self,
            mode,
            index: 0,
            pending: None,
        };
        let value = visitor.visit_map(&mut access)?;
        if access.pending.is_some() || access.index != access.length() {
            return Err(AsonbError::Message(
                "Visitor did not consume the complete map.".into(),
            ));
        }
        Ok(value)
    }
}

macro_rules! deserialize_numbers {($($method:ident,$variant:ident,$visit:ident,$ty:expr);* $(;)?)=>{$(fn $method<V:Visitor<'de>>(self,visitor:V)->Result<V::Value,AsonbError>{if self.ty()? != $ty{return Err(self.mismatch(stringify!($variant)));}let Value::$variant(value)=self.value()? else{unreachable!()};visitor.$visit(value)})*};}

impl<'de, I: Input<'de>> de::Deserializer<'de> for &mut Deserializer<'_, 'de, I> {
    type Error = AsonbError;
    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, AsonbError> {
        Err(self.mismatch("a concrete Deserialize target (deserialize_any is unsupported)"))
    }
    deserialize_numbers!(deserialize_i8,I8,visit_i8,Type::I8;deserialize_i16,I16,visit_i16,Type::I16;deserialize_i32,I32,visit_i32,Type::I32;deserialize_i64,I64,visit_i64,Type::I64;deserialize_u8,U8,visit_u8,Type::U8;deserialize_u16,U16,visit_u16,Type::U16;deserialize_u32,U32,visit_u32,Type::U32;deserialize_u64,U64,visit_u64,Type::U64;deserialize_f32,F32,visit_f32,Type::F32;deserialize_f64,F64,visit_f64,Type::F64;deserialize_bool,Bool,visit_bool,Type::Bool;deserialize_char,Char,visit_char,Type::Char);
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let Type::String(encoding) = self.ty()? else {
            return Err(self.mismatch("a String"));
        };
        if matches!(encoding, Encoding::Fixed(_) | Encoding::LongFixed(_)) {
            return Err(self
                .mismatch("an application-interpreted fixed String slot (use the low-level API)"));
        }
        match read_blob(self.input, &encoding, true, self.root)? {
            Cow::Borrowed(bytes) => visitor.visit_borrowed_str(
                std::str::from_utf8(bytes)
                    .map_err(|error| AsonbError::Message(error.to_string()))?,
            ),
            Cow::Owned(bytes) => visitor.visit_string(
                String::from_utf8(bytes).map_err(|error| AsonbError::Message(error.to_string()))?,
            ),
        }
    }
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let Type::String(encoding) = self.ty()? else {
            return Err(self.mismatch("a String"));
        };
        if matches!(encoding, Encoding::Fixed(_) | Encoding::LongFixed(_)) {
            return Err(self.mismatch("an application-interpreted fixed String slot"));
        }
        let bytes = read_blob(self.input, &encoding, true, self.root)?;
        visitor.visit_string(
            String::from_utf8(bytes.into_owned())
                .map_err(|error| AsonbError::Message(error.to_string()))?,
        )
    }
    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let Type::Bytes(encoding) = self.ty()? else {
            return Err(self.mismatch("Byte Data"));
        };
        match read_blob(self.input, &encoding, false, self.root)? {
            Cow::Borrowed(bytes) => visitor.visit_borrowed_bytes(bytes),
            Cow::Owned(bytes) => visitor.visit_byte_buf(bytes),
        }
    }
    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let Type::Bytes(encoding) = self.ty()? else {
            return Err(self.mismatch("Byte Data"));
        };
        let bytes = read_blob(self.input, &encoding, false, self.root)?;
        visitor.visit_byte_buf(bytes.into_owned())
    }
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let (_, variant) = self.enumeration()?;
        match variant {
            Variant::Unit(name) if name == "None" => visitor.visit_none(),
            Variant::Newtype(name, ty) if name == "Some" => {
                visitor.visit_some(&mut Deserializer::new(self.input, self.schema, ty, false))
            }
            _ => Err(self.mismatch("None or Some")),
        }
    }
    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        if self.ty()? != Type::Tuple(vec![]) {
            return Err(self.mismatch("an empty Tuple"));
        }
        visitor.visit_unit()
    }
    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        match self.ty()? {
            Type::Object(_, fields) if fields.is_empty() => visitor.visit_unit(),
            _ => Err(self.mismatch("an empty Object")),
        }
    }
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        if name == crate::date_time::MARKER {
            if self.ty()? != Type::DateTime {
                return Err(self.mismatch("DateTime"));
            }
            self.ty = Type::Tuple(vec![Type::I64, Type::U32, Type::I32]);
        }
        visitor.visit_newtype_struct(self)
    }
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        self.sequence(visitor, None)
    }
    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        self.sequence(visitor, Some(length))
    }
    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        self.deserialize_tuple(length, visitor)
    }
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        self.mapping(visitor, false)
    }
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        self.mapping(visitor, true)
    }
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        let (_, variant) = self.enumeration()?;
        visitor.visit_enum(VariantAccessor {
            deserializer: self,
            variant,
        })
    }
    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        self.deserialize_str(visitor)
    }
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        skip_value(self.input, self.schema, &self.ty)?;
        visitor.visit_unit()
    }
    fn is_human_readable(&self) -> bool {
        false
    }
}

enum SequenceMode {
    List {
        encoding: Encoding,
        item: Type,
        remaining: u64,
        ended: bool,
    },
    Tuple(Vec<Type>, usize),
}
struct SeqAccessor<'a, 'w, 'de, I: Input<'de>> {
    deserializer: &'a mut Deserializer<'w, 'de, I>,
    mode: SequenceMode,
}
impl<'de, I: Input<'de>> SeqAccessor<'_, '_, 'de, I> {
    fn complete(&self) -> bool {
        match &self.mode {
            SequenceMode::Tuple(types, index) => *index == types.len(),
            SequenceMode::List {
                encoding,
                remaining,
                ended,
                ..
            } => *remaining == 0 && (*encoding != Encoding::MultiChunk || *ended),
        }
    }
}
impl<'de, I: Input<'de>> SeqAccess<'de> for SeqAccessor<'_, '_, 'de, I> {
    type Error = AsonbError;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, AsonbError> {
        let ty = match &mut self.mode {
            SequenceMode::Tuple(types, index) => {
                let Some(ty) = types.get(*index).cloned() else {
                    return Ok(None);
                };
                *index += 1;
                ty
            }
            SequenceMode::List {
                encoding,
                item,
                remaining,
                ended,
            } => {
                if *remaining == 0 {
                    if *encoding != Encoding::MultiChunk || *ended {
                        return Ok(None);
                    }
                    *remaining = self.deserializer.input.chunk(self.deserializer.root)? as u64;
                    if *remaining == 0 {
                        *ended = true;
                        return Ok(None);
                    }
                }
                *remaining -= 1;
                item.clone()
            }
        };
        seed.deserialize(&mut Deserializer::new(
            self.deserializer.input,
            self.deserializer.schema,
            ty,
            false,
        ))
        .map(Some)
    }
}

enum MappingMode {
    Object(Vec<Field>),
    Map { key: Type, value: Type, count: u64 },
}
struct MapAccessor<'a, 'w, 'de, I: Input<'de>> {
    deserializer: &'a mut Deserializer<'w, 'de, I>,
    mode: MappingMode,
    index: u64,
    pending: Option<Type>,
}
impl<'de, I: Input<'de>> MapAccessor<'_, '_, 'de, I> {
    fn length(&self) -> u64 {
        match &self.mode {
            MappingMode::Object(fields) => fields.len() as u64,
            MappingMode::Map { count, .. } => *count,
        }
    }
}
impl<'de, I: Input<'de>> MapAccess<'de> for MapAccessor<'_, '_, 'de, I> {
    type Error = AsonbError;
    fn next_key_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, AsonbError> {
        if self.pending.is_some() {
            return Err(AsonbError::Message("Map value was not consumed.".into()));
        }
        if self.index >= self.length() {
            return Ok(None);
        }
        match &self.mode {
            MappingMode::Object(fields) => {
                let field = &fields[self.index as usize];
                self.pending = Some(field.ty.clone());
                seed.deserialize(field.name.clone().into_deserializer())
                    .map(Some)
            }
            MappingMode::Map { key, value, .. } => {
                self.pending = Some(value.clone());
                seed.deserialize(&mut Deserializer::new(
                    self.deserializer.input,
                    self.deserializer.schema,
                    key.clone(),
                    false,
                ))
                .map(Some)
            }
        }
    }
    fn next_value_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<T::Value, AsonbError> {
        let ty = self
            .pending
            .take()
            .ok_or_else(|| AsonbError::Message("Map key was not consumed.".into()))?;
        self.index += 1;
        seed.deserialize(&mut Deserializer::new(
            self.deserializer.input,
            self.deserializer.schema,
            ty,
            false,
        ))
    }
}

struct VariantAccessor<'a, 'w, 'de, I: Input<'de>> {
    deserializer: &'a mut Deserializer<'w, 'de, I>,
    variant: Variant,
}
impl<'de, I: Input<'de>> EnumAccess<'de> for VariantAccessor<'_, '_, 'de, I> {
    type Error = AsonbError;
    type Variant = Self;
    fn variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<(T::Value, Self), AsonbError> {
        let value = seed.deserialize(self.variant.name().to_owned().into_deserializer())?;
        Ok((value, self))
    }
}
impl<'de, I: Input<'de>> VariantAccess<'de> for VariantAccessor<'_, '_, 'de, I> {
    type Error = AsonbError;
    fn unit_variant(self) -> Result<(), AsonbError> {
        if !matches!(self.variant, Variant::Unit(_)) {
            return Err(AsonbError::Message(
                "Expected a variant without a payload.".into(),
            ));
        }
        Ok(())
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, AsonbError> {
        let Variant::Newtype(_, ty) = self.variant else {
            return Err(AsonbError::Message(
                "Expected a single-value variant.".into(),
            ));
        };
        seed.deserialize(&mut Deserializer::new(
            self.deserializer.input,
            self.deserializer.schema,
            ty,
            false,
        ))
    }
    fn tuple_variant<V: Visitor<'de>>(
        self,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        if !matches!(self.variant, Variant::Tuple(..)) {
            return Err(AsonbError::Message("Expected a tuple-like variant.".into()));
        }
        let ty = payload_type(&self.variant);
        de::Deserializer::deserialize_tuple(
            &mut Deserializer::new(self.deserializer.input, self.deserializer.schema, ty, false),
            length,
            visitor,
        )
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        if !matches!(self.variant, Variant::Struct(..)) {
            return Err(AsonbError::Message(
                "Expected an object-like variant.".into(),
            ));
        }
        let ty = payload_type(&self.variant);
        de::Deserializer::deserialize_struct(
            &mut Deserializer::new(self.deserializer.input, self.deserializer.schema, ty, false),
            "",
            fields,
            visitor,
        )
    }
}

/// Incremental root List reader. It is fused after its marker, EOF, or first
/// error, and never probes beyond an explicit marker for a trailing document.
pub struct ListStreamReader<T, R: Read> {
    input: ReaderInput<R>,
    schema: DocumentSchema,
    item: Type,
    remaining: u64,
    finished: bool,
    marker: PhantomData<fn() -> T>,
}
pub fn list_stream_reader<T: de::DeserializeOwned, R: Read>(
    reader: R,
) -> Result<ListStreamReader<T, R>, AsonbError> {
    let mut input = ReaderInput::new(reader);
    input.header()?;
    let schema = DocumentSchema::read(&mut input)?;
    let Type::List(Encoding::MultiChunk, item) = schema.resolve(schema.root.clone())? else {
        return Err(AsonbError::Message(
            "List streaming requires a multi-chunk root List.".into(),
        ));
    };
    Ok(ListStreamReader {
        input,
        schema,
        item: *item,
        remaining: 0,
        finished: false,
        marker: PhantomData,
    })
}
impl<T: de::DeserializeOwned, R: Read> Iterator for ListStreamReader<T, R> {
    type Item = Result<T, AsonbError>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let result = (|| {
            if self.remaining == 0 {
                self.remaining = self.input.chunk(true)? as u64;
                if self.remaining == 0 {
                    return Ok(None);
                }
            }
            self.remaining -= 1;
            T::deserialize(&mut Deserializer::new(
                &mut self.input,
                &self.schema,
                self.item.clone(),
                false,
            ))
            .map(Some)
        })();
        match result {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => {
                self.finished = true;
                None
            }
            Err(error) => {
                self.finished = true;
                Some(Err(error))
            }
        }
    }
}
impl<T: de::DeserializeOwned, R: Read> FusedIterator for ListStreamReader<T, R> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ser::{list_stream_writer, ser_to_bytes};
    #[test]
    fn test_boolean_and_type_errors() {
        for byte in 0u8..=255 {
            let mut bytes = ser_to_bytes(&false).unwrap();
            bytes[13] = byte;
            assert_eq!(de_from_slice::<bool>(&bytes).unwrap(), byte != 0);
        }
        assert!(de_from_slice::<i64>(&ser_to_bytes(&1i32).unwrap()).is_err());
        let mut bytes = ser_to_bytes(&1i32).unwrap();
        bytes.push(0);
        assert!(de_from_slice::<i32>(&bytes).is_err());
    }
    #[test]
    fn test_borrowed_strings() {
        let bytes = ser_to_bytes(&"hello").unwrap();
        let value = de_from_slice::<&str>(&bytes).unwrap();
        assert_eq!(value, "hello");
        assert_eq!(value.as_ptr(), bytes[17..].as_ptr());
        let value = de_from_reader::<String, _>(bytes.as_slice()).unwrap();
        assert_eq!(value, "hello");
    }
    #[test]
    fn test_stream_end_and_error() {
        let mut bytes = Vec::new();
        let mut writer = list_stream_writer::<i32, _>(&mut bytes);
        writer.start_list().unwrap();
        writer.write(&11).unwrap();
        writer.write(&13).unwrap();
        writer.end_list().unwrap();
        let mut source = bytes.as_slice();
        let mut reader = list_stream_reader::<i32, _>(&mut source).unwrap();
        assert_eq!(reader.next().unwrap().unwrap(), 11);
        assert_eq!(reader.next().unwrap().unwrap(), 13);
        assert!(reader.next().is_none());
        assert!(reader.next().is_none());
        let truncated = &bytes[..bytes.len() - 5];
        let mut reader = list_stream_reader::<i32, _>(truncated).unwrap();
        assert_eq!(reader.next().unwrap().unwrap(), 11);
        assert!(reader.next().unwrap().is_err());
        assert!(reader.next().is_none());
    }

    macro_rules! integer_test {
        ($name:ident,$rust:ty,$ty:expr) => {
            #[test]
            fn $name() {
                for value in [<$rust>::MIN, 0, <$rust>::MAX] {
                    let bytes = crate::test_utils::document($ty, &value.to_le_bytes());
                    assert_eq!(de_from_slice::<$rust>(&bytes).unwrap(), value);
                    assert_eq!(de_from_reader::<$rust, _>(bytes.as_slice()).unwrap(), value);
                    for end in 0..bytes.len() {
                        assert!(de_from_slice::<$rust>(&bytes[..end]).is_err());
                    }
                }
            }
        };
    }
    integer_test!(test_i8, i8, Type::I8);
    integer_test!(test_i16, i16, Type::I16);
    integer_test!(test_i32, i32, Type::I32);
    integer_test!(test_i64, i64, Type::I64);
    integer_test!(test_u8, u8, Type::U8);
    integer_test!(test_u16, u16, Type::U16);
    integer_test!(test_u32, u32, Type::U32);
    integer_test!(test_u64, u64, Type::U64);

    #[test]
    fn test_floats_and_characters() {
        for bits in [
            0u32, 0x80000000, 1, 0x7f800000, 0xff800000, 0x7fc00011, 0xffc00011,
        ] {
            let bytes = crate::test_utils::document(Type::F32, &bits.to_le_bytes());
            assert_eq!(de_from_slice::<f32>(&bytes).unwrap().to_bits(), bits);
        }
        for bits in [
            0u64,
            0x8000000000000000,
            1,
            0x7ff0000000000000,
            0xfff0000000000000,
            0x7ff8000000000011,
        ] {
            let bytes = crate::test_utils::document(Type::F64, &bits.to_le_bytes());
            assert_eq!(de_from_slice::<f64>(&bytes).unwrap().to_bits(), bits);
        }
        for scalar in [0u32, 0x42, 0x6587, 0xd7ff, 0xe000, 0x10ffff] {
            let bytes = crate::test_utils::document(Type::Char, &scalar.to_le_bytes());
            assert_eq!(de_from_slice::<char>(&bytes).unwrap() as u32, scalar);
        }
        for scalar in [0xd800u32, 0xdfff, 0x110000, u32::MAX] {
            assert!(
                de_from_slice::<char>(&crate::test_utils::document(
                    Type::Char,
                    &scalar.to_le_bytes()
                ))
                .is_err()
            );
        }
    }

    #[test]
    fn test_byte_borrowing_and_chunks() {
        let bytes =
            crate::test_utils::document(Type::Bytes(Encoding::Regular), &[3, 0, 0, 0, 0xff, 0, 1]);
        let value = de_from_slice::<&serde_bytes::Bytes>(&bytes).unwrap();
        assert_eq!(value.as_ptr(), bytes[17..].as_ptr());
        assert_eq!(value.as_ref(), [0xff, 0, 1]);
        let bytes = crate::test_utils::document(
            Type::String(Encoding::MultiChunk),
            &[1, 0, 0, 0, b'a', 1, 0, 0, 0, b'b', 0, 0, 0, 0],
        );
        assert_eq!(de_from_slice::<String>(&bytes).unwrap(), "ab");
        assert!(de_from_slice::<&str>(&bytes).is_err());
        let bytes = crate::test_utils::document(
            Type::String(Encoding::MultiChunk),
            &[1, 0, 0, 0, 0xc3, 1, 0, 0, 0, 0xa9, 0, 0, 0, 0],
        );
        assert!(de_from_slice::<String>(&bytes).is_err());
        let bytes =
            crate::test_utils::document(Type::String(Encoding::Long), &u64::MAX.to_le_bytes());
        assert!(de_from_slice::<String>(&bytes).is_err());
    }

    #[test]
    fn test_short_reads_and_io_errors() {
        let bytes = ser_to_bytes(&vec!["foo", "bar"]).unwrap();
        let reader = crate::test_utils::FaultReader::new(&bytes);
        assert_eq!(
            de_from_reader::<Vec<String>, _>(reader).unwrap(),
            ["foo", "bar"]
        );
        for fail_at in 0..bytes.len() {
            let mut reader = crate::test_utils::FaultReader::new(&bytes);
            reader.fail_at = fail_at;
            assert!(de_from_reader::<Vec<String>, _>(reader).is_err());
        }
    }

    #[test]
    fn test_stream_does_not_read_ahead() {
        let mut bytes = Vec::new();
        let mut writer = list_stream_writer::<i32, _>(&mut bytes);
        writer.start_list().unwrap();
        writer.write(&11).unwrap();
        writer.end_list().unwrap();
        let boundary = bytes.len();
        bytes.extend_from_slice(&ser_to_bytes(&13i32).unwrap());
        let mut source = bytes.as_slice();
        {
            let mut reader = list_stream_reader::<i32, _>(&mut source).unwrap();
            assert_eq!(reader.next().unwrap().unwrap(), 11);
            assert!(reader.next().is_none());
        }
        assert_eq!(source, &bytes[boundary..]);
        assert_eq!(de_from_reader::<i32, _>(source).unwrap(), 13);
    }
}
