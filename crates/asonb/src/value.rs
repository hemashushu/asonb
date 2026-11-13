// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Explicit values for applications that select their own wire schema.

use std::borrow::Cow;

/// A value interpreted according to a separate document schema.
///
/// Container values do not repeat field names, element types, or encoding modes:
/// those belong to [`DocumentSchema`](crate::DocumentSchema). Strings and bytes may borrow from a slice
/// document; containers own their child storage. Enumeration indices refer to
/// schema order, not Rust discriminant values.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'de> {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Char(char),
    DateTime(i64),
    String(Cow<'de, str>),
    Bytes(Cow<'de, [u8]>),
    List(Vec<Value<'de>>),
    Map(Vec<(Value<'de>, Value<'de>)>),
    Tuple(Vec<Value<'de>>),
    Object(Vec<Value<'de>>),
    /// An enumeration variant selected by its index in the schema.
    ///
    /// The variant names belong to the schema; the payload shape must match
    /// the selected [`Variant`](crate::Variant).
    Enumeration {
        /// The variant's position in the schema's variant list.
        index: u32,
        /// The variant's unit, newtype, tuple, or struct payload.
        payload: VariantValue<'de>,
    },
}

/// The payload carried by an enumeration variant.
///
/// Each shape corresponds to the [`Variant`](crate::Variant) of the same name.
#[derive(Debug, Clone, PartialEq)]
pub enum VariantValue<'de> {
    /// A variant without a payload.
    Unit,
    /// A variant with one value.
    Newtype(Box<Value<'de>>),
    /// A variant with positional values, in schema order.
    Tuple(Vec<Value<'de>>),
    /// A variant with named fields, whose values are in schema order.
    Struct(Vec<Value<'de>>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DocumentSchema, Encoding, Type, Variant,
        binary::{HEADER, write_all},
        de::{de_value_from_reader, de_value_from_slice},
        ser::ser_value_to_writer,
    };

    /// Checks both ownership modes and truncations, except legal root chunk EOFs.
    fn round_trip(ty: Type, value: Value<'_>) {
        let schema = DocumentSchema::new(ty);
        let mut bytes = Vec::new();
        ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
        assert_eq!(de_value_from_slice(&bytes).unwrap().1, value);
        assert_eq!(de_value_from_reader(bytes.as_slice()).unwrap().1, value);
        for end in 0..bytes.len() {
            if matches!(
                schema.root,
                Type::List(Encoding::MultiChunk, _)
                    | Type::String(Encoding::MultiChunk)
                    | Type::Bytes(Encoding::MultiChunk)
            ) {
                continue;
            }
            assert!(de_value_from_slice(&bytes[..end]).is_err());
        }
    }

    #[test]
    /// Covers all framing modes and all four enumeration payload shapes.
    fn test_every_payload_form() {
        for encoding in [Encoding::Regular, Encoding::Long, Encoding::MultiChunk] {
            round_trip(
                Type::String(encoding.clone()),
                Value::String(Cow::Borrowed("a\0b")),
            );
            round_trip(
                Type::Bytes(encoding.clone()),
                Value::Bytes(Cow::Borrowed(&[0xff, 0, 3])),
            );
            round_trip(
                Type::List(encoding, Box::new(Type::I32)),
                Value::List(vec![
                    Value::I32(i32::MIN),
                    Value::I32(0),
                    Value::I32(i32::MAX),
                ]),
            );
        }
        for long in [false, true] {
            round_trip(
                Type::Map(long, Box::new(Type::U8), Box::new(Type::Bool)),
                Value::Map(vec![(Value::U8(1), Value::Bool(true))]),
            );
        }
        round_trip(Type::DateTime, Value::DateTime(i64::MIN));
        let variants = vec![
            Variant::Unit("A".into()),
            Variant::Newtype("B".into(), Type::I8),
            Variant::Tuple("C".into(), vec![Type::Bool, Type::I32]),
            Variant::Struct("D".into(), vec![crate::Field::new("x", Type::Char)]),
        ];
        for (index, payload) in [
            VariantValue::Unit,
            VariantValue::Newtype(Box::new(Value::I8(-1))),
            VariantValue::Tuple(vec![Value::Bool(true), Value::I32(42)]),
            VariantValue::Struct(vec![Value::Char('\u{10ffff}')]),
        ]
        .into_iter()
        .enumerate()
        {
            round_trip(
                Type::Enum("E".into(), variants.clone()),
                Value::Enumeration {
                    index: index as u32,
                    payload,
                },
            );
        }
        // The payload shape must match the selected variant, not just its wire type.
        let schema = DocumentSchema::new(Type::Enum("E".into(), variants));
        for value in [
            Value::Enumeration {
                index: 0,
                payload: VariantValue::Tuple(vec![]),
            },
            Value::Enumeration {
                index: 2,
                payload: VariantValue::Newtype(Box::new(Value::Tuple(vec![
                    Value::Bool(true),
                    Value::I32(42),
                ]))),
            },
            Value::Enumeration {
                index: 3,
                payload: VariantValue::Tuple(vec![Value::Char('x')]),
            },
            Value::Enumeration {
                index: 2,
                payload: VariantValue::Tuple(vec![Value::Bool(true)]),
            },
        ] {
            assert!(ser_value_to_writer(&schema, &value, &mut Vec::new()).is_err());
        }
    }

    #[test]
    /// Allows root EOF only at chunk boundaries and rejects invalid text chunks.
    fn test_chunk_boundaries_and_utf8() {
        let schema = DocumentSchema::new(Type::String(Encoding::MultiChunk));
        let mut bytes = Vec::new();
        write_all(&mut bytes, &HEADER).unwrap();
        schema.write(&mut bytes).unwrap();
        let start = bytes.len();
        bytes.extend_from_slice(&[1, 0, 0, 0, b'a']);
        assert_eq!(
            de_value_from_slice(&bytes).unwrap().1,
            Value::String(Cow::Borrowed("a"))
        );
        for end in start + 1..start + 5 {
            assert!(de_value_from_slice(&bytes[..end]).is_err());
        }
        bytes[start + 4] = 0xff;
        assert!(de_value_from_slice(&bytes).is_err());
    }

    #[test]
    /// Rejects non-scalar Unicode values and mismatched dynamic numeric widths.
    fn test_invalid_values() {
        let schema = DocumentSchema::new(Type::Char);
        let mut bytes = Vec::new();
        write_all(&mut bytes, &HEADER).unwrap();
        schema.write(&mut bytes).unwrap();
        for value in [0xd800u32, 0xdfff, 0x110000, u32::MAX] {
            let mut input = bytes.clone();
            input.extend_from_slice(&value.to_le_bytes());
            assert!(de_value_from_slice(&input).is_err());
        }
        assert!(
            ser_value_to_writer(
                &DocumentSchema::new(Type::I32),
                &Value::I64(1),
                &mut Vec::new()
            )
            .is_err()
        );
    }

    #[test]
    /// Distinguishes optional root terminators from mandatory nested terminators.
    fn test_nested_chunks_and_empty_values() {
        for (ty, value) in [
            (Type::String(Encoding::MultiChunk), Value::String("".into())),
            (
                Type::Bytes(Encoding::MultiChunk),
                Value::Bytes(Cow::Borrowed(&[])),
            ),
            (
                Type::List(Encoding::MultiChunk, Box::new(Type::I32)),
                Value::List(vec![]),
            ),
        ] {
            let mut bytes = Vec::new();
            ser_value_to_writer(&DocumentSchema::new(ty), &value, &mut bytes).unwrap();
            assert_eq!(de_value_from_slice(&bytes).unwrap().1, value);
            bytes.truncate(bytes.len() - 4);
            assert_eq!(de_value_from_slice(&bytes).unwrap().1, value);
            bytes.push(0);
            assert!(de_value_from_slice(&bytes).is_err());
        }
        let schema = DocumentSchema::new(Type::List(
            Encoding::Regular,
            Box::new(Type::String(Encoding::MultiChunk)),
        ));
        let mut bytes = Vec::new();
        ser_value_to_writer(
            &schema,
            &Value::List(vec![Value::String("a".into())]),
            &mut bytes,
        )
        .unwrap();
        assert!(de_value_from_slice(&bytes).is_ok());
        bytes.truncate(bytes.len() - 4);
        assert!(de_value_from_slice(&bytes).is_err());
    }
}
