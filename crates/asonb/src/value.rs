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
/// those belong to [`DocumentSchema`](crate::DocumentSchema). This is a dynamic
/// wire-value model, not a Serde data model: use [`crate::ser_value_to_writer`]
/// and the `de_value_*` functions rather than the typed Serde entry points.
/// Every variant must match its schema exactly; numeric widths and container
/// shapes are not converted implicitly.
///
/// The lifetime `'de` applies only to borrowed string and byte leaves.
/// [`crate::de_value_from_slice`] borrows Regular and Long payloads, whereas
/// MultiChunk payloads are concatenated into owned storage. Reader-based APIs
/// return `Value<'static>` with owned leaves. All container vectors are owned;
/// constructing a value does not validate it against a schema.
///
/// # Examples
///
/// Object values contain only the fields' values, in schema order:
///
/// ```
/// use asonb::{DocumentSchema, Encoding, Field, Type, Value};
///
/// let schema = DocumentSchema::new(Type::Object(vec![
///     Field::new("id", Type::U32),
///     Field::new("name", Type::String(Encoding::Regular)),
/// ]));
/// let value = Value::Object(vec![Value::U32(7), Value::String("sensor".into())]);
/// let mut bytes = Vec::new();
/// asonb::ser_value_to_writer(&schema, &value, &mut bytes)?;
/// assert_eq!(asonb::de_value_from_slice(&bytes)?, (schema, value));
/// # Ok::<(), asonb::AsonbError>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'de> {
    /// A signed 8-bit integer; matches [`crate::Type::I8`].
    I8(i8),
    /// A signed 16-bit integer; matches [`crate::Type::I16`].
    I16(i16),
    /// A signed 32-bit integer; matches [`crate::Type::I32`].
    I32(i32),
    /// A signed 64-bit integer; matches [`crate::Type::I64`].
    I64(i64),
    /// An unsigned 8-bit integer; matches [`crate::Type::U8`].
    U8(u8),
    /// An unsigned 16-bit integer; matches [`crate::Type::U16`].
    U16(u16),
    /// An unsigned 32-bit integer; matches [`crate::Type::U32`].
    U32(u32),
    /// An unsigned 64-bit integer; matches [`crate::Type::U64`].
    U64(u64),
    /// An IEEE 754 single-precision value; writing canonicalizes NaN payload bits.
    F32(f32),
    /// An IEEE 754 double-precision value; writing canonicalizes NaN payload bits.
    F64(f64),
    /// A Boolean; readers interpret any nonzero wire byte as `true`.
    Bool(bool),
    /// A Unicode scalar value, not an arbitrary 32-bit character code.
    Char(char),
    /// Signed whole seconds since the Unix epoch, without an offset.
    ///
    /// All `i64` values are representable here, even outside chrono's supported
    /// range. This matches [`crate::Type::DateTime`], not [`crate::Type::I64`].
    DateTime(i64),
    /// UTF-8 text with framing supplied by [`crate::Type::String`].
    ///
    /// Borrowed text points into the input document, not its schema.
    String(Cow<'de, str>),
    /// Raw Byte Data with framing supplied by [`crate::Type::Bytes`].
    ///
    /// This is distinct from a [`Self::List`] containing [`Self::U8`] elements.
    Bytes(Cow<'de, [u8]>),
    /// Homogeneous elements matching the schema's List element type.
    ///
    /// The schema, rather than the first element, defines an empty List's type.
    /// Original chunk boundaries are not retained in the materialized vector.
    List(Vec<Value<'de>>),
    /// Key/value pairs in wire order, preserving duplicate keys.
    ///
    /// All keys share the schema's key type; all values share its value type.
    /// Unlike a Rust map collection, this vector imposes no key uniqueness rule.
    Map(Vec<(Value<'de>, Value<'de>)>),
    /// Positional values matching each Tuple type in order and in number.
    ///
    /// A document Tuple must contain at least one element.
    Tuple(Vec<Value<'de>>),
    /// Field values in the order of the schema's nonempty Object field list.
    ///
    /// Names are stored only in the schema. Every schema field needs one value;
    /// the vector cannot omit fields or supply its own field names.
    Object(Vec<Value<'de>>),
    /// An enumeration variant selected by its index in the schema.
    ///
    /// The variant names belong to the schema; the payload shape must match
    /// the selected [`Variant`](crate::Variant).
    Enumeration {
        /// The zero-based position in the schema's variant list, not a Rust
        /// discriminant or a variant-form tag.
        index: u32,
        /// The variant's unit, newtype, tuple, or struct payload.
        payload: VariantValue<'de>,
    },
}

/// The payload carried by an enumeration variant.
///
/// Each shape corresponds to the [`Variant`](crate::Variant) of the same name
/// and is checked when writing a [`Value::Enumeration`]. Shapes are not
/// interchangeable: a one-value newtype is not a one-element tuple, and a struct
/// payload is not an Object wrapped in a newtype.
///
/// # Examples
///
/// ```
/// use asonb::{DocumentSchema, Type, Value, Variant, VariantValue};
///
/// let schema = DocumentSchema::new(Type::Enum("Status".into(), vec![
///     Variant::Unit("Idle".into()),
///     Variant::Newtype("Count".into(), Type::U32),
/// ]));
/// let value = Value::Enumeration {
///     index: 1,
///     payload: VariantValue::Newtype(Box::new(Value::U32(3))),
/// };
/// let mut bytes = Vec::new();
/// asonb::ser_value_to_writer(&schema, &value, &mut bytes)?;
/// assert_eq!(asonb::de_value_from_slice(&bytes)?.1, value);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum VariantValue<'de> {
    /// A variant without a payload.
    Unit,
    /// A variant with exactly one value matching its newtype payload schema.
    Newtype(Box<Value<'de>>),
    /// At least two positional values matching the variant's types in order.
    Tuple(Vec<Value<'de>>),
    /// A nonempty set of field values in schema order; names are in the schema.
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
