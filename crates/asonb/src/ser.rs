// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use crate::{
    AsonbError, DocumentSchema, Encoding, Field, Schema, Type, Value, Variant,
    binary::{HEADER, ValueBuffer, write_all, write_u32},
    value::{payload_type, write_length, write_value},
};
use serde::{Serialize, ser};
use std::{io::Write, marker::PhantomData};

/// Serialize a document without requiring a seekable writer. The schema is
/// built before emitting bytes, including for empty containers.
pub fn ser_to_writer<T: Serialize + Schema + ?Sized, W: Write>(
    value: &T,
    writer: &mut W,
) -> Result<(), AsonbError> {
    let schema = DocumentSchema::of::<T>()?;
    write_all(writer, &HEADER)?;
    schema.write(writer)?;
    value.serialize(&mut Serializer::new(writer, &schema, schema.root.clone()))
}

pub fn ser_to_bytes<T: Serialize + Schema + ?Sized>(value: &T) -> Result<Vec<u8>, AsonbError> {
    let mut bytes = Vec::new();
    ser_to_writer(value, &mut bytes)?;
    Ok(bytes)
}

pub fn list_stream_writer<T: Schema, W: Write>(writer: &mut W) -> ListStreamWriter<'_, W, T> {
    ListStreamWriter {
        writer,
        schema: None,
        state: StreamState::NotStarted,
        marker: PhantomData,
    }
}

/// A schema-guided Serde writer. Child serializers borrow the same output,
/// but have their own type and nesting state, so nested containers do not
/// overwrite the framing state of their parents.
pub struct Serializer<'a, W: Write> {
    writer: &'a mut W,
    schema: &'a DocumentSchema,
    ty: Type,
    nanoseconds: bool,
}

impl<'a, W: Write> Serializer<'a, W> {
    pub(crate) fn new(writer: &'a mut W, schema: &'a DocumentSchema, ty: Type) -> Self {
        Self {
            writer,
            schema,
            ty,
            nanoseconds: false,
        }
    }
    fn ty(&self) -> Result<Type, AsonbError> {
        self.schema.resolve(self.ty.clone())
    }
    fn scalar(&mut self, value: Value<'_>) -> Result<(), AsonbError> {
        write_value(self.writer, self.schema, &self.ty, &value)
    }
    fn variant(&mut self, name: &str, form: u8) -> Result<Type, AsonbError> {
        let Type::Enum(_, variants) = self.ty()? else {
            return Err(AsonbError::Message(
                "Expected an Enumeration schema.".into(),
            ));
        };
        let (index, variant) = variants
            .iter()
            .enumerate()
            .find(|(_, variant)| variant.name() == name)
            .ok_or_else(|| AsonbError::Message(format!("Unknown variant {name}.")))?;
        let actual = match variant {
            Variant::Unit(_) => 0,
            Variant::Newtype(_, _) => 1,
            Variant::Tuple(_, _) => 2,
            Variant::Struct(_, _) => 4,
        };
        if actual != form {
            return Err(AsonbError::Message(
                "Variant payload form does not match schema.".into(),
            ));
        }
        write_u32(self.writer, index as u64)?;
        Ok(payload_type(variant))
    }
}

enum Kind {
    Sequence(Encoding, Type),
    Map(bool, Type, Type),
    Tuple(Vec<Type>),
    Object(Vec<Field>),
    DateTime,
}

/// Per-container count and ordering validation. Only an unknown-length regular
/// sequence/map buffers its own payload to put the count before the bytes.
pub struct Compound<'a, 'w, W: Write> {
    serializer: &'a mut Serializer<'w, W>,
    kind: Kind,
    count: usize,
    expected: Option<usize>,
    pending: bool,
    buffer: Option<ValueBuffer>,
}

impl<'a, 'w, W: Write> Compound<'a, 'w, W> {
    fn new(
        serializer: &'a mut Serializer<'w, W>,
        ty: Type,
        length: Option<usize>,
    ) -> Result<Self, AsonbError> {
        let mut buffer = None;
        let kind = match ty {
            Type::List(encoding, item) => {
                if encoding != Encoding::MultiChunk {
                    if let Some(length) = length {
                        write_length(serializer.writer, &encoding, length as u64)?;
                    } else if matches!(encoding, Encoding::Regular | Encoding::Long) {
                        buffer = Some(ValueBuffer { bytes: Vec::new() });
                    } else {
                        return Err(AsonbError::Message(
                            "Fixed List requires a known count.".into(),
                        ));
                    }
                }
                Kind::Sequence(encoding, *item)
            }
            Type::Map(long, key, value) => {
                if let Some(length) = length {
                    write_length(
                        serializer.writer,
                        &if long {
                            Encoding::Long
                        } else {
                            Encoding::Regular
                        },
                        length as u64,
                    )?;
                } else {
                    buffer = Some(ValueBuffer { bytes: Vec::new() });
                }
                Kind::Map(long, *key, *value)
            }
            Type::Tuple(types) => {
                if length != Some(types.len()) {
                    return Err(AsonbError::Message(
                        "Tuple arity does not match schema.".into(),
                    ));
                }
                Kind::Tuple(types)
            }
            Type::Object(_, fields) => {
                if length != Some(fields.len()) {
                    return Err(AsonbError::Message(
                        "Object field count does not match schema.".into(),
                    ));
                }
                Kind::Object(fields)
            }
            Type::DateTime if length == Some(3) => Kind::DateTime,
            _ => {
                return Err(AsonbError::Message(
                    "Container does not match schema.".into(),
                ));
            }
        };
        Ok(Self {
            serializer,
            kind,
            count: 0,
            expected: length,
            pending: false,
            buffer,
        })
    }

    fn child<T: Serialize + ?Sized>(
        &mut self,
        ty: Type,
        value: &T,
        nanoseconds: bool,
    ) -> Result<(), AsonbError> {
        let serializer = &mut self.serializer;
        let result = if let Some(buffer) = &mut self.buffer {
            value.serialize(&mut Serializer::new(buffer, serializer.schema, ty))
        } else {
            let mut child = Serializer::new(serializer.writer, serializer.schema, ty);
            child.nanoseconds = nanoseconds;
            value.serialize(&mut child)
        };
        result?;
        Ok(())
    }

    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        if self.expected.is_some_and(|expected| self.count >= expected) {
            return Err(AsonbError::Message("Too many elements.".into()));
        }
        let ty = match &self.kind {
            Kind::Sequence(encoding, ty) => {
                if *encoding == Encoding::MultiChunk {
                    write_u32(self.serializer.writer, 1)?;
                }
                ty.clone()
            }
            Kind::Tuple(types) => types
                .get(self.count)
                .cloned()
                .ok_or_else(|| AsonbError::Message("Too many tuple elements.".into()))?,
            Kind::DateTime => match self.count {
                0 => Type::I64,
                1 => Type::U32,
                2 => Type::I32,
                _ => return Err(AsonbError::Message("Too many DateTime fields.".into())),
            },
            _ => return Err(AsonbError::Message("Expected a sequence.".into())),
        };
        self.child(
            ty,
            value,
            matches!(self.kind, Kind::DateTime) && self.count == 1,
        )?;
        self.count += 1;
        Ok(())
    }

    fn field<T: Serialize + ?Sized>(&mut self, name: &str, value: &T) -> Result<(), AsonbError> {
        let Kind::Object(fields) = &self.kind else {
            return Err(AsonbError::Message("Expected Object fields.".into()));
        };
        let field = fields
            .get(self.count)
            .ok_or_else(|| AsonbError::Message("Too many Object fields.".into()))?;
        if field.name != name {
            return Err(AsonbError::Message(format!(
                "Expected field {}, found {name}.",
                field.name
            )));
        }
        self.child(field.ty.clone(), value, false)?;
        self.count += 1;
        Ok(())
    }

    fn key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        if self.pending {
            return Err(AsonbError::Message("Map value is missing.".into()));
        }
        if self.expected.is_some_and(|expected| self.count >= expected) {
            return Err(AsonbError::Message("Too many Map pairs.".into()));
        }
        let Kind::Map(_, key, _) = &self.kind else {
            return Err(AsonbError::Message("Expected Map.".into()));
        };
        self.child(key.clone(), value, false)?;
        self.pending = true;
        Ok(())
    }

    fn map_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        if !self.pending {
            return Err(AsonbError::Message("Map key is missing.".into()));
        }
        let Kind::Map(_, _, ty) = &self.kind else {
            return Err(AsonbError::Message("Expected Map.".into()));
        };
        self.child(ty.clone(), value, false)?;
        self.pending = false;
        self.count += 1;
        Ok(())
    }

    fn finish(self) -> Result<(), AsonbError> {
        if self.pending || self.expected.is_some_and(|expected| expected != self.count) {
            return Err(AsonbError::Message("Incomplete container.".into()));
        }
        if let Some(buffer) = self.buffer {
            let encoding = match self.kind {
                Kind::Map(long, _, _) => {
                    if long {
                        Encoding::Long
                    } else {
                        Encoding::Regular
                    }
                }
                Kind::Sequence(encoding, _) => encoding,
                _ => unreachable!(),
            };
            write_length(self.serializer.writer, &encoding, self.count as u64)?;
            write_all(self.serializer.writer, &buffer.bytes)?;
        } else if matches!(self.kind, Kind::Sequence(Encoding::MultiChunk, _)) {
            write_u32(self.serializer.writer, 0)?;
        }
        Ok(())
    }
}

macro_rules! serialize_numbers {($($method:ident,$rust:ty,$variant:ident);* $(;)?)=>{$(fn $method(self,value:$rust)->Result<(),AsonbError>{self.scalar(Value::$variant(value))})*};}

impl<'a, 'w, W: Write> ser::Serializer for &'a mut Serializer<'w, W> {
    type Ok = ();
    type Error = AsonbError;
    type SerializeSeq = Compound<'a, 'w, W>;
    type SerializeTuple = Compound<'a, 'w, W>;
    type SerializeTupleStruct = Compound<'a, 'w, W>;
    type SerializeTupleVariant = Compound<'a, 'w, W>;
    type SerializeMap = Compound<'a, 'w, W>;
    type SerializeStruct = Compound<'a, 'w, W>;
    type SerializeStructVariant = Compound<'a, 'w, W>;
    serialize_numbers!(serialize_i8,i8,I8;serialize_i16,i16,I16;serialize_i32,i32,I32;serialize_i64,i64,I64;serialize_u8,u8,U8;serialize_u16,u16,U16;serialize_u64,u64,U64;serialize_f32,f32,F32;serialize_f64,f64,F64;serialize_bool,bool,Bool;serialize_char,char,Char);
    fn serialize_u32(self, value: u32) -> Result<(), AsonbError> {
        if self.nanoseconds && value >= 1_000_000_000 {
            return Err(AsonbError::Message("Invalid nanoseconds.".into()));
        }
        self.scalar(Value::U32(value))
    }
    fn serialize_str(self, value: &str) -> Result<(), AsonbError> {
        self.scalar(Value::String(value.into()))
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<(), AsonbError> {
        self.scalar(Value::Bytes(value.into()))
    }
    fn serialize_none(self) -> Result<(), AsonbError> {
        self.variant("None", 0)?;
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), AsonbError> {
        let ty = self.variant("Some", 1)?;
        value.serialize(&mut Serializer::new(self.writer, self.schema, ty))
    }
    fn serialize_unit(self) -> Result<(), AsonbError> {
        Err(AsonbError::Message("Unit is unsupported.".into()))
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), AsonbError> {
        Err(AsonbError::Message("Unit structs are unsupported.".into()))
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<(), AsonbError> {
        self.variant(variant, 0)?;
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<(), AsonbError> {
        if name == crate::date_time::MARKER {
            if self.ty()? != Type::DateTime {
                return Err(AsonbError::Message("Expected DateTime schema.".into()));
            }
            value.serialize(self)
        } else {
            Err(AsonbError::Message(
                "Newtype structs are unsupported.".into(),
            ))
        }
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), AsonbError> {
        let ty = self.variant(variant, 1)?;
        value.serialize(&mut Serializer::new(self.writer, self.schema, ty))
    }
    fn serialize_seq(self, length: Option<usize>) -> Result<Self::SerializeSeq, AsonbError> {
        let ty = self.ty()?;
        if !matches!(ty, Type::List(..)) {
            return Err(AsonbError::Message("Expected List schema.".into()));
        }
        Compound::new(self, ty, length)
    }
    fn serialize_tuple(self, length: usize) -> Result<Self::SerializeTuple, AsonbError> {
        let ty = self.ty()?;
        if !matches!(ty, Type::Tuple(_) | Type::DateTime) {
            return Err(AsonbError::Message("Expected Tuple schema.".into()));
        }
        Compound::new(self, ty, Some(length))
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct, AsonbError> {
        Err(AsonbError::Message("Tuple structs are unsupported.".into()))
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        length: usize,
    ) -> Result<Self::SerializeTupleVariant, AsonbError> {
        let ty = self.variant(variant, 2)?;
        Compound::new(self, ty, Some(length))
    }
    fn serialize_map(self, length: Option<usize>) -> Result<Self::SerializeMap, AsonbError> {
        let ty = self.ty()?;
        if !matches!(ty, Type::Map(..)) {
            return Err(AsonbError::Message("Expected Map schema.".into()));
        }
        Compound::new(self, ty, length)
    }
    fn serialize_struct(
        self,
        _name: &'static str,
        length: usize,
    ) -> Result<Self::SerializeStruct, AsonbError> {
        let ty = self.ty()?;
        if !matches!(ty, Type::Object(..)) {
            return Err(AsonbError::Message("Expected Object schema.".into()));
        }
        Compound::new(self, ty, Some(length))
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        length: usize,
    ) -> Result<Self::SerializeStructVariant, AsonbError> {
        let ty = self.variant(variant, 4)?;
        Compound::new(self, ty, Some(length))
    }
    fn is_human_readable(&self) -> bool {
        false
    }
}

macro_rules! sequence_impl {
    ($trait:ident,$method:ident) => {
        impl<W: Write> ser::$trait for Compound<'_, '_, W> {
            type Ok = ();
            type Error = AsonbError;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
                self.element(value)
            }
            fn end(self) -> Result<(), AsonbError> {
                self.finish()
            }
        }
    };
}
sequence_impl!(SerializeSeq, serialize_element);
sequence_impl!(SerializeTuple, serialize_element);
sequence_impl!(SerializeTupleStruct, serialize_field);
sequence_impl!(SerializeTupleVariant, serialize_field);
macro_rules! object_impl {
    ($trait:ident) => {
        impl<W: Write> ser::$trait for Compound<'_, '_, W> {
            type Ok = ();
            type Error = AsonbError;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), AsonbError> {
                self.field(key, value)
            }
            fn end(self) -> Result<(), AsonbError> {
                self.finish()
            }
        }
    };
}
object_impl!(SerializeStruct);
object_impl!(SerializeStructVariant);
impl<W: Write> ser::SerializeMap for Compound<'_, '_, W> {
    type Ok = ();
    type Error = AsonbError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        self.key(value)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        self.map_value(value)
    }
    fn end(self) -> Result<(), AsonbError> {
        self.finish()
    }
}

#[derive(PartialEq, Eq)]
enum StreamState {
    NotStarted,
    Open,
    Finished,
    Failed,
}

/// A forward-only root List writer. Drop does not write an end marker.
pub struct ListStreamWriter<'a, W: Write, T> {
    writer: &'a mut W,
    schema: Option<DocumentSchema>,
    state: StreamState,
    marker: PhantomData<fn(&T)>,
}

impl<W: Write, T: Schema> ListStreamWriter<'_, W, T> {
    pub fn start_list(&mut self) -> Result<(), AsonbError> {
        if self.state != StreamState::NotStarted {
            return Err(AsonbError::Message(
                "List stream has already started.".into(),
            ));
        }
        self.state = StreamState::Failed;
        let mut schema = DocumentSchema::of::<T>()?;
        schema.root = Type::List(Encoding::MultiChunk, Box::new(schema.root));
        write_all(self.writer, &HEADER)?;
        schema.write(self.writer)?;
        self.schema = Some(schema);
        self.state = StreamState::Open;
        Ok(())
    }

    /// Writes a single value as one chunk in the root list.
    /// Returns an error if the list is not open.
    pub fn write(&mut self, value: &T) -> Result<(), AsonbError>
    where
        T: Serialize,
    {
        if self.state != StreamState::Open {
            return Err(AsonbError::Message("List stream is not open.".into()));
        }
        self.state = StreamState::Failed;
        let schema = self
            .schema
            .as_ref()
            .ok_or_else(|| AsonbError::Message("Missing stream schema.".into()))?;
        let Type::List(_, item) = &schema.root else {
            unreachable!()
        };
        write_u32(self.writer, 1)?;
        value.serialize(&mut Serializer::new(self.writer, schema, (**item).clone()))?;
        self.state = StreamState::Open;
        Ok(())
    }

    /// Writes multiple values as one chunk in the root list.
    /// Returns an error if the list is not open.
    pub fn writes(&mut self, values: &[T]) -> Result<(), AsonbError>
    where
        T: Serialize,
    {
        if self.state != StreamState::Open {
            return Err(AsonbError::Message("List stream is not open.".into()));
        }
        self.state = StreamState::Failed;
        let schema = self
            .schema
            .as_ref()
            .ok_or_else(|| AsonbError::Message("Missing stream schema.".into()))?;
        let Type::List(_, item) = &schema.root else {
            unreachable!()
        };
        // An empty slice writes nothing, since a zero count is the end marker.
        if !values.is_empty() {
            write_u32(self.writer, values.len() as u64)?;
            for value in values {
                value.serialize(&mut Serializer::new(self.writer, schema, (**item).clone()))?;
            }
        }
        self.state = StreamState::Open;
        Ok(())
    }

    /// Ends the root list by writing an explicit end marker.
    /// Returns an error if the list is not open.
    pub fn end_list(&mut self) -> Result<(), AsonbError> {
        if self.state != StreamState::Open {
            return Err(AsonbError::Message("List stream is not open.".into()));
        }
        self.state = StreamState::Failed;
        write_u32(self.writer, 0)?;
        self.state = StreamState::Finished;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_primitive_bytes() {
        assert_eq!(
            ser_to_bytes(&0x42i32).unwrap(),
            [
                0, 65, 83, 79, 78, 66, 0, 1, // header
                0, 0, 0, 0,    // reference type count
                0x15, // root value type
                0x42, 0, 0, 0 // root value
            ]
        );
        assert_eq!(
            &ser_to_bytes(&true).unwrap()[12..],
            &[
                0x40, // root value type: Boolean
                1
            ]
        );
        assert_eq!(
            &ser_to_bytes(&"a\0b").unwrap()[12..],
            &[
                0x50, // root value type: Regular String
                3, 0, 0, 0, b'a', 0, b'b'
            ]
        );
        assert_eq!(
            &ser_to_bytes(&f32::NAN).unwrap()[13..],
            &0x7fc00000u32.to_le_bytes()
        );
    }
    #[test]
    fn test_stream_states() {
        let mut output = Vec::new();
        let mut stream = list_stream_writer::<i32, _>(&mut output);
        assert!(stream.write(&1).is_err());

        stream.start_list().unwrap();
        assert!(stream.start_list().is_err());

        stream.write(&11).unwrap();
        stream.end_list().unwrap();

        assert!(stream.write(&2).is_err());
        assert!(stream.end_list().is_err());

        assert_eq!(&output[14..], &[1, 0, 0, 0, 11, 0, 0, 0, 0, 0, 0, 0]);

        let mut output = Vec::new();
        let mut stream = list_stream_writer::<i32, _>(&mut output);
        assert!(stream.writes(&[1]).is_err());
        stream.start_list().unwrap();
        stream.writes(&[11, 22]).unwrap();
        stream.writes(&[]).unwrap();
        stream.write(&33).unwrap();
        stream.end_list().unwrap();
        assert_eq!(
            &output[14..],
            &[
                2, 0, 0, 0, 11, 0, 0, 0, 22, 0, 0, 0, // chunk of 2
                1, 0, 0, 0, 33, 0, 0, 0, // chunk of 1
                0, 0, 0, 0 // end marker
            ]
        );
        assert_eq!(
            crate::de_from_slice::<Vec<i32>>(&output).unwrap(),
            [11, 22, 33]
        );
    }

    macro_rules! integer_test {
        ($name:ident,$rust:ty) => {
            #[test]
            fn $name() {
                for value in [<$rust>::MIN, 0, <$rust>::MAX] {
                    let bytes = ser_to_bytes(&value).unwrap();
                    assert_eq!(&bytes[13..], &value.to_le_bytes());
                    assert_eq!(crate::de_from_slice::<$rust>(&bytes).unwrap(), value);
                }
            }
        };
    }
    integer_test!(test_i8, i8);
    integer_test!(test_i16, i16);
    integer_test!(test_i32, i32);
    integer_test!(test_i64, i64);
    integer_test!(test_u8, u8);
    integer_test!(test_u16, u16);
    integer_test!(test_u32, u32);
    integer_test!(test_u64, u64);

    #[test]
    fn test_floating_point_bits() {
        for value in [
            0.0f32,
            -0.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert_eq!(&ser_to_bytes(&value).unwrap()[13..], &value.to_le_bytes());
        }
        for value in [
            0.0f64,
            -0.0,
            f64::MIN,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(&ser_to_bytes(&value).unwrap()[13..], &value.to_le_bytes());
        }
        assert_eq!(
            &ser_to_bytes(&f64::from_bits(0xfff8000000000011)).unwrap()[13..],
            &0x7ff8000000000000u64.to_le_bytes()
        );
    }

    #[test]
    fn test_character_bytes() {
        for value in ['\0', 'B', '文', '\u{d7ff}', '\u{e000}', '\u{10ffff}'] {
            assert_eq!(
                &ser_to_bytes(&value).unwrap()[13..],
                &(value as u32).to_le_bytes()
            );
        }
    }

    struct UnknownLength;
    impl Schema for UnknownLength {
        fn schema(builder: &mut crate::SchemaBuilder) -> Result<Type, AsonbError> {
            Vec::<String>::schema(builder)
        }
    }
    impl Serialize for UnknownLength {
        fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use ser::SerializeSeq;
            let mut seq = serializer.serialize_seq(None)?;
            seq.serialize_element("foo")?;
            seq.serialize_element("bar")?;
            seq.end()
        }
    }

    #[test]
    fn test_unknown_length_buffer() {
        let bytes = ser_to_bytes(&UnknownLength).unwrap();
        assert_eq!(
            crate::de_from_slice::<Vec<String>>(&bytes).unwrap(),
            ["foo", "bar"]
        );
        let mut output = Vec::new();
        ser_to_writer(&UnknownLength, &mut output).unwrap();
        assert_eq!(output, bytes);
    }

    #[test]
    fn test_partial_and_failed_writes() {
        use crate::test_utils::FaultWriter;
        let mut writer = FaultWriter::default();
        ser_to_writer(&"hello", &mut writer).unwrap();
        assert_eq!(writer.bytes, ser_to_bytes(&"hello").unwrap());
        for fail_at in 0..18 {
            let mut writer = FaultWriter {
                fail_at,
                ..FaultWriter::default()
            };
            assert!(ser_to_writer(&"hello", &mut writer).is_err());
        }
        let mut writer = FaultWriter {
            zero: true,
            ..FaultWriter::default()
        };
        assert!(ser_to_writer(&1i32, &mut writer).is_err());
        let mut writer = FaultWriter {
            fail_at: 15,
            ..FaultWriter::default()
        };
        let mut stream = list_stream_writer::<i32, _>(&mut writer);
        stream.start_list().unwrap();
        assert!(stream.write(&1).is_err());
        assert!(stream.write(&2).is_err());
        assert!(stream.end_list().is_err());
    }
}
