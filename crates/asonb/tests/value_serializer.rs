// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Tests for the public dynamic `Value` serializer and deserializer.

use asonb::{
    DocumentSchema, Encoding, Field, Type, Value, Variant, VariantValue, de_from_slice,
    ser_to_bytes, ser_value_to_writer,
};
use pretty_assertions::assert_eq;
use std::borrow::Cow;

#[test]
/// Checks an empty dynamic List still retains its explicitly selected i32 type.
fn test_empty_list_preserves_explicit_item_schema() {
    let schema = DocumentSchema::new(Type::List(Encoding::Regular, Box::new(Type::I32)));
    let value = Value::List(vec![]);
    let mut bytes = Vec::new();
    ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
    assert_eq!(asonb::de_value_from_slice(&bytes).unwrap(), (schema, value));
}

#[test]
/// Distinguishes truncated wire payloads from chrono's narrower timestamp range.
fn test_malformed_input() {
    let schema = DocumentSchema::new(Type::List(Encoding::Long, Box::new(Type::I32)));
    let mut bytes = Vec::new();
    ser_value_to_writer(&schema, &Value::List(vec![]), &mut bytes).unwrap();
    let end = bytes.len();
    // Keep a valid long prefix but promise elements that are not present. This
    // must be a truncation error, not an arbitrary regular-length policy limit.
    bytes[end - 8..].copy_from_slice(&(u32::MAX as u64 + 1).to_le_bytes());
    assert!(matches!(
        asonb::de_value_from_slice(&bytes),
        Err(asonb::AsonbError::UnexpectedEndOfDocument(_))
    ));
    let schema = DocumentSchema::new(Type::DateTime);
    let mut bytes = Vec::new();
    ser_value_to_writer(&schema, &Value::DateTime(0), &mut bytes).unwrap();
    assert!(de_from_slice::<asonb::date_time::DateTime>(&bytes).is_ok());
    assert!(asonb::de_value_from_slice(&bytes[..20]).is_err());
    let schema = DocumentSchema::new(Type::DateTime);
    let mut bytes = Vec::new();
    ser_value_to_writer(&schema, &Value::DateTime(i64::MAX), &mut bytes).unwrap();
    assert!(asonb::de_value_from_slice(&bytes).is_ok());
    assert!(de_from_slice::<asonb::date_time::DateTime>(&bytes).is_err());
}

#[test]
/// Covers every explicit framing mode and preserves ordered duplicate Map keys.
fn test_value_encodings_round_trip() {
    for encoding in [Encoding::Regular, Encoding::Long, Encoding::MultiChunk] {
        let cases = [
            (Type::String(encoding.clone()), Value::String("a文".into())),
            (
                Type::Bytes(encoding.clone()),
                Value::Bytes(Cow::Borrowed(&[0xff, 0, 1, 2])),
            ),
            (
                Type::List(encoding, Box::new(Type::I32)),
                Value::List(vec![
                    Value::I32(1),
                    Value::I32(2),
                    Value::I32(3),
                    Value::I32(4),
                ]),
            ),
        ];
        for (ty, value) in cases {
            let schema = DocumentSchema::new(ty);
            let mut bytes = Vec::new();
            ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
            assert_eq!(asonb::de_value_from_slice(&bytes).unwrap().1, value);
            assert_eq!(
                asonb::de_value_from_reader(bytes.as_slice()).unwrap().1,
                value
            );
        }
    }
    for long in [false, true] {
        let schema = DocumentSchema::new(Type::Map(long, Box::new(Type::U8), Box::new(Type::I32)));
        let value = Value::Map(vec![
            (Value::U8(2), Value::I32(11)),
            (Value::U8(1), Value::I32(13)),
            (Value::U8(2), Value::I32(17)),
        ]);
        let mut bytes = Vec::new();
        ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
        assert_eq!(asonb::de_value_from_slice(&bytes).unwrap().1, value);
    }
    let schema = DocumentSchema::new(Type::Tuple(vec![Type::I32, Type::String(Encoding::Long)]));
    let value = Value::Tuple(vec![Value::I32(-7), Value::String("tuple".into())]);
    let mut bytes = Vec::new();
    ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
    assert_eq!(asonb::de_value_from_slice(&bytes).unwrap(), (schema, value));
}

#[test]
/// Rejects dynamic values whose shape disagrees with the explicit schema.
fn test_value_writer_rejects_schema_mismatches() {
    let event = Type::Enum(
        "Event".into(),
        vec![
            Variant::Unit("Idle".into()),
            Variant::Tuple("Move".into(), vec![Type::I32, Type::I32]),
        ],
    );
    let cases = [
        (Type::I32, Value::I64(1), "Value does not match"),
        (
            Type::String(Encoding::Regular),
            Value::Bytes(Cow::Borrowed(b"text")),
            "String or byte-data value does not match",
        ),
        (
            Type::Tuple(vec![Type::I32, Type::I32]),
            Value::Tuple(vec![Value::I32(1)]),
            "Tuple arity mismatch",
        ),
        (
            Type::Object(vec![Field::new("id", Type::I32)]),
            Value::Object(vec![]),
            "Object field count mismatch",
        ),
        (
            event.clone(),
            Value::Enumeration {
                index: 2,
                payload: VariantValue::Unit,
            },
            "Variant index out of range",
        ),
        (
            event.clone(),
            Value::Enumeration {
                index: 0,
                payload: VariantValue::Newtype(Box::new(Value::I32(1))),
            },
            "Payload does not match variant \"Idle\"",
        ),
        (
            event,
            Value::Enumeration {
                index: 1,
                payload: VariantValue::Tuple(vec![Value::I32(1)]),
            },
            "Payload arity mismatch for variant \"Move\"",
        ),
    ];
    for (ty, value, expected) in cases {
        let mut bytes = Vec::new();
        let error = ser_value_to_writer(&DocumentSchema::new(ty), &value, &mut bytes).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn test_finite_readers_reject_truncation_and_trailing_bytes() {
    let bytes = ser_to_bytes(&vec![(11i32, "first"), (13, "second")]).unwrap();
    for end in 0..bytes.len() {
        let input = &bytes[..end];
        assert!(asonb::de_value_from_slice(input).is_err(), "end {end}");
        assert!(asonb::de_value_from_reader(input).is_err(), "end {end}");
        assert!(
            de_from_slice::<Vec<(i32, String)>>(input).is_err(),
            "end {end}"
        );
        assert!(
            asonb::de_from_reader::<Vec<(i32, String)>, _>(input).is_err(),
            "end {end}"
        );
    }

    let mut suffixed = bytes;
    suffixed.extend_from_slice(&ser_to_bytes(&true).unwrap());
    assert!(asonb::de_value_from_slice(&suffixed).is_err());
    assert!(asonb::de_value_from_reader(suffixed.as_slice()).is_err());
    assert!(de_from_slice::<Vec<(i32, String)>>(&suffixed).is_err());
    assert!(asonb::de_from_reader::<Vec<(i32, String)>, _>(suffixed.as_slice()).is_err());
}

#[test]
/// Keeps deep-but-acyclic schemas valid without an artificial nesting quota.
fn test_unrestricted_schema_round_trip() {
    let mut ty = Type::I32;
    let mut value = Value::I32(11);
    for _ in 0..160 {
        ty = Type::List(Encoding::Regular, Box::new(ty));
        value = Value::List(vec![value]);
    }
    let nested_schema = DocumentSchema::new(ty);
    let mut bytes = Vec::new();
    ser_value_to_writer(&nested_schema, &value, &mut bytes).unwrap();
    assert_eq!(
        asonb::de_value_from_slice(&bytes).unwrap(),
        (nested_schema.clone(), value.clone())
    );
    assert_eq!(
        asonb::de_value_from_reader(bytes.as_slice()).unwrap(),
        (nested_schema, value)
    );
}

proptest::proptest! {
    #[test]
    fn test_arbitrary_inputs_never_panic(
        bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..256),
    ) {
        let _ = asonb::de_value_from_slice(&bytes);
        let _ = de_from_slice::<Vec<String>>(&bytes);

        // Random documents rarely pass the signature; valid schemas reach payload decoding.
        for (ty, seed, payload_length) in [
            (Type::Char, Value::Char('x'), 4),
            (Type::String(Encoding::Regular), Value::String("".into()), 4),
            (Type::Bytes(Encoding::Long), Value::Bytes(Cow::Borrowed(&[])), 8),
            (Type::List(Encoding::MultiChunk, Box::new(Type::Bool)), Value::List(vec![]), 4),
            (Type::Map(false, Box::new(Type::U8), Box::new(Type::I32)), Value::Map(vec![]), 4),
            (
                Type::Object(vec![Field::new("text", Type::String(Encoding::MultiChunk))]),
                Value::Object(vec![Value::String("".into())]),
                4,
            ),
            (
                Type::Enum("Event".into(), vec![Variant::Newtype("Data".into(), Type::I64)]),
                Value::Enumeration {
                    index: 0,
                    payload: VariantValue::Newtype(Box::new(Value::I64(0))),
                },
                12,
            ),
        ] {
            let mut document = Vec::new();
            ser_value_to_writer(&DocumentSchema::new(ty), &seed, &mut document).unwrap();
            document.truncate(document.len() - payload_length);
            document.extend_from_slice(&bytes);
            let _ = de_from_slice::<Vec<String>>(&document);
            let from_slice = asonb::de_value_from_slice(&document);
            // Validate lengths against the slice before allowing a reader to allocate them.
            if let Ok(expected) = from_slice {
                let actual = asonb::de_value_from_reader(document.as_slice()).unwrap();
                proptest::prop_assert_eq!(actual, expected);
            }
        }
    }
}
