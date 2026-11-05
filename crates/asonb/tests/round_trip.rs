// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! End-to-end contracts across derived schemas, Serde adapters, and raw values.
//! These tests deliberately mix APIs so a writer and its matching reader cannot
//! hide the same mistake behind an apparently successful round trip.

use asonb::{
    DocumentSchema, Encoding, Field, Schema, Type, Value, Variant, de_from_reader, de_from_slice,
    ser_to_bytes, ser_value_to_writer,
};
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
};

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
/// The README's small Object example, exercised through both input modes.
struct Package {
    /// Regular String field used to check named-object serialization.
    name: String,
    /// A second String field, ensuring schema order is preserved.
    version: String,
    /// A homogeneous List whose type is available even when it is empty.
    dependencies: Vec<String>,
}

#[test]
/// Keeps the introductory example compatible with slice and reader decoding.
fn test_readme_package() {
    let value = Package {
        name: "foo".into(),
        version: "0.1.0".into(),
        dependencies: vec!["random".into(), "regex".into()],
    };
    let bytes = ser_to_bytes(&value).unwrap();
    assert_eq!(de_from_slice::<Package>(&bytes).unwrap(), value);
    assert_eq!(
        de_from_reader::<Package, _>(bytes.as_slice()).unwrap(),
        value
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
/// Covers all four supported enumeration payload shapes in one reusable fixture.
enum Event {
    /// Unit branch: the index is the entire payload.
    None,
    /// Newtype branch: exactly one typed value follows the index.
    Number(
        /// Signed test payload.
        i32,
    ),
    /// Tuple branch: a positional, heterogeneous payload.
    Pair(
        /// Text, including possible embedded NUL.
        String,
        /// Logical flag.
        bool,
    ),
    /// Struct branch: field types and names are carried by the variant schema.
    Object {
        /// Unsigned identifier, tested at the maximum wire value.
        id: u64,
        /// Text field that may be empty without losing its schema.
        name: String,
    },
}

#[test]
/// Checks every variant plus empty List/Map and unselected Option/Result branches.
fn test_enum_variants_and_empty_containers() {
    let values = vec![
        Event::None,
        Event::Number(i32::MIN),
        Event::Pair("文\0".into(), true),
        Event::Object {
            id: u64::MAX,
            name: "".into(),
        },
    ];
    assert_eq!(
        de_from_slice::<Vec<Event>>(&ser_to_bytes(&values).unwrap()).unwrap(),
        values
    );
    let empty: Vec<Event> = vec![];
    let none: Option<Event> = None;
    assert_eq!(
        de_from_slice::<Vec<Event>>(&ser_to_bytes(&empty).unwrap()).unwrap(),
        empty
    );
    assert_eq!(
        de_from_slice::<Option<Event>>(&ser_to_bytes(&none).unwrap()).unwrap(),
        none
    );
    for value in [Ok::<Event, String>(Event::Number(42)), Err("error".into())] {
        assert_eq!(
            de_from_slice::<Result<Event, String>>(&ser_to_bytes(&value).unwrap()).unwrap(),
            value
        );
    }
    let map = HashMap::<String, Option<Event>>::new();
    assert_eq!(
        de_from_slice::<HashMap<String, Option<Event>>>(&ser_to_bytes(&map).unwrap()).unwrap(),
        map
    );
}

#[test]
/// Checks an empty dynamic List still retains its explicitly selected i32 type.
fn test_empty_list_i32_fallback_type_round_trip() {
    let schema = DocumentSchema::new(Type::List(Encoding::Regular, Box::new(Type::I32)));
    let value = Value::List(vec![]);
    let mut bytes = Vec::new();
    ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
    assert_eq!(asonb::de_value_from_slice(&bytes).unwrap(), (schema, value));
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(rename_all = "camelCase")]
/// Combines generic child schemas, field casing, and a nonserialized field.
struct Renamed<T> {
    /// Becomes firstName in the schema and in Serde's serialized field name.
    first_name: String,
    /// The generic payload whose Schema bound is supplied by the derive.
    value: T,
    #[serde(skip)]
    /// Excluded from the wire and restored to its default on deserialization.
    ignored: u32,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(rename_all = "snake_case")]
/// Guards Serde's character-by-character acronym casing behavior.
enum Acronym {
    /// Must serialize as h_t_t_p_server, not http_server.
    HTTPServer,
    /// Ordinary PascalCase comparison case.
    OtherValue,
}

#[test]
/// Checks derived casing, skipped fields, transparent wrappers, and const generics.
fn test_serde_attributes_and_generics() {
    let value = Renamed {
        first_name: "Alice".into(),
        value: vec![1i32, 2],
        ignored: 0,
    };
    assert_eq!(
        de_from_slice::<Renamed<Vec<i32>>>(&ser_to_bytes(&value).unwrap()).unwrap(),
        value
    );
    assert_eq!(
        de_from_slice::<Acronym>(&ser_to_bytes(&Acronym::HTTPServer).unwrap()).unwrap(),
        Acronym::HTTPServer
    );
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(transparent)]
    /// Named wrapper whose wire schema is its field's scalar type.
    struct Transparent {
        /// The sole serialized field; no Object metadata should surround it.
        value: u32,
    }
    assert_eq!(
        de_from_slice::<Transparent>(&ser_to_bytes(&Transparent { value: 3 }).unwrap()).unwrap(),
        Transparent { value: 3 }
    );
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(bound(
        serialize = "[u8; SIZE]: Serialize",
        deserialize = "[u8; SIZE]: Deserialize<'de>"
    ))]
    /// Shows that Schema's const-generic support still needs Serde's array bounds.
    struct ConstGeneric<const SIZE: usize> {
        /// A default array mapping, represented as a positional Tuple.
        value: [u8; SIZE],
    }
    let value = ConstGeneric { value: [1, 2, 3] };
    assert_eq!(
        de_from_slice::<ConstGeneric<3>>(&ser_to_bytes(&value).unwrap()).unwrap(),
        value
    );
}

#[test]
/// Checks field-name matching, defaults, unknown-field policy, and enum reordering.
fn test_field_and_variant_name_resolution() {
    #[derive(Debug, PartialEq, Deserialize)]
    /// Intentionally differs from the writer's order and descriptive object name.
    struct Target {
        /// Appears after id on the wire, but before id in this Rust structure.
        name: String,
        /// Matched by name, not by its Rust field position.
        id: i32,
        #[serde(default)]
        /// Missing on the wire and therefore supplied by Serde's default.
        active: bool,
    }
    let schema = DocumentSchema::new(Type::Object(
        "UnrelatedMetadata".into(),
        vec![
            Field::new("id", Type::I32),
            Field::new("extra", Type::List(Encoding::Regular, Box::new(Type::I32))),
            Field::new("name", Type::String(Encoding::Regular)),
        ],
    ));
    let value = Value::Object(vec![
        Value::I32(11),
        Value::List(vec![Value::I32(2)]),
        Value::String("Alice".into()),
    ]);
    let mut bytes = Vec::new();
    ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
    assert_eq!(
        de_from_slice::<Target>(&bytes).unwrap(),
        Target {
            name: "Alice".into(),
            id: 11,
            active: false
        }
    );
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    /// Same known fields, but refuses the writer's additional extra field.
    struct Strict {
        /// Known identifier field.
        id: i32,
        /// Known text field.
        name: String,
    }
    assert!(de_from_slice::<Strict>(&bytes).is_err());
    let schema = DocumentSchema::new(Type::Enum(
        "Event".into(),
        vec![
            Variant::Newtype("Number".into(), Type::I32),
            Variant::Unit("None".into()),
        ],
    ));
    let mut bytes = Vec::new();
    ser_value_to_writer(
        &schema,
        &Value::Enum(0, Box::new(Value::I32(9))),
        &mut bytes,
    )
    .unwrap();
    assert_eq!(de_from_slice::<Event>(&bytes).unwrap(), Event::Number(9));
}

#[test]
/// Checks binary DateTime/Byte Data adapters independently of their Rust types.
fn test_date_time_and_bytes_adapters() {
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    /// Fields whose adapters intentionally replace their default Serde mappings.
    struct Record {
        #[serde(with = "asonb::date_time")]
        /// Encodes UTC seconds, discarding the original display offset.
        time: chrono::DateTime<chrono::FixedOffset>,
        #[serde(with = "serde_bytes")]
        /// Byte Data rather than the List<u8> normally used by Vec<u8>.
        data: Vec<u8>,
    }
    let instant = chrono::DateTime::from_timestamp(-1, 0).unwrap();
    let value = Record {
        time: instant.with_timezone(&chrono::FixedOffset::east_opt(28_800).unwrap()),
        data: vec![0xff, 0, 1],
    };
    let bytes = ser_to_bytes(&value).unwrap();
    assert_eq!(
        de_from_slice::<Record>(&bytes).unwrap(),
        Record {
            time: value
                .time
                .with_timezone(&chrono::FixedOffset::east_opt(0).unwrap()),
            data: value.data.clone(),
        }
    );
    let time = asonb::date_time::DateTime(value.time);
    assert_eq!(
        de_from_slice::<asonb::date_time::DateTime>(&ser_to_bytes(&time).unwrap()).unwrap(),
        time
    );
    let byte_value = serde_bytes::ByteBuf::from(vec![0xff, 0, 1]);
    assert_eq!(
        de_from_slice::<serde_bytes::ByteBuf>(&ser_to_bytes(&byte_value).unwrap()).unwrap(),
        byte_value
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
/// Makes array representation choices visible in one derived Object schema.
struct ArrayMappings {
    /// Default Serde array behavior: a Tuple, even though its items are bytes.
    default_bytes: [u8; 3],
    /// A second default Tuple mapping with a different element type and arity.
    default_values: [i32; 4],
    #[serde(with = "asonb::array")]
    /// Explicit homogeneous List<u8>, not Byte Data.
    byte_list: [u8; 3],
    #[serde(with = "asonb::array")]
    /// Exercises adapter support beyond Serde's built-in small-array derives.
    large_list: [i32; 40],
    #[serde(with = "asonb::array")]
    /// Zero-length List is valid, unlike a zero-element document Tuple.
    empty_list: [u16; 0],
    #[serde(with = "serde_bytes")]
    /// Explicit Byte Data rather than either an array Tuple or a List.
    byte_data: [u8; 3],
}

#[test]
/// Verifies payload round trips and inspects the actual derived wire types.
fn test_array_serde_mappings() {
    let value = ArrayMappings {
        default_bytes: [1, 2, 3],
        default_values: [11, 13, 17, 19],
        byte_list: [4, 5, 6],
        large_list: std::array::from_fn(|index| index as i32),
        empty_list: [],
        byte_data: [7, 8, 9],
    };
    let bytes = ser_to_bytes(&value).unwrap();
    assert_eq!(de_from_slice::<ArrayMappings>(&bytes).unwrap(), value);

    let schema = DocumentSchema::of::<ArrayMappings>().unwrap();
    let object = schema
        .references
        .iter()
        .find_map(|ty| match ty {
            Type::Object(_, fields) => Some(fields),
            _ => None,
        })
        .unwrap();
    let field_type = |name| &object.iter().find(|field| field.name == name).unwrap().ty;
    assert!(matches!(field_type("default_bytes"), Type::Reference(_)));
    assert!(matches!(field_type("default_values"), Type::Reference(_)));
    assert_eq!(
        field_type("byte_list"),
        &Type::List(Encoding::Regular, Box::new(Type::U8))
    );
    assert_eq!(
        field_type("large_list"),
        &Type::List(Encoding::Regular, Box::new(Type::I32))
    );
    assert_eq!(
        field_type("empty_list"),
        &Type::List(Encoding::Regular, Box::new(Type::U16))
    );
    assert_eq!(field_type("byte_data"), &Type::Bytes(Encoding::Regular));
}

#[test]
/// Requires the List adapter to reject both undersized and oversized payloads.
fn test_array_list_adapter_rejects_wrong_length() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    /// The target size is deliberately stricter than the constructed input Lists.
    struct TwoItems {
        #[serde(with = "asonb::array")]
        /// Exactly two values are required, not just at least two.
        values: [i32; 2],
    }

    for values in [
        vec![Value::I32(1)],
        vec![Value::I32(1), Value::I32(2), Value::I32(3)],
    ] {
        let schema = DocumentSchema::new(Type::Object(
            "".into(),
            vec![Field::new(
                "values",
                Type::List(Encoding::Regular, Box::new(Type::I32)),
            )],
        ));
        let mut bytes = Vec::new();
        ser_value_to_writer(
            &schema,
            &Value::Object(vec![Value::List(values)]),
            &mut bytes,
        )
        .unwrap();
        assert!(de_from_slice::<TwoItems>(&bytes).is_err());
    }
}

#[test]
/// Checks that borrowed fields point inside the original document allocation.
fn test_borrowed_fields() {
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    /// A zero-copy target whose fields must not borrow temporary decoder storage.
    struct Borrowed<'a> {
        /// Contiguous regular String bytes borrowed as text.
        text: &'a str,
        #[serde(borrow)]
        /// Contiguous Byte Data borrowed through serde_bytes' wrapper.
        data: &'a serde_bytes::Bytes,
    }
    let value = Borrowed {
        text: "文\0",
        data: serde_bytes::Bytes::new(&[1, 0, 0xff]),
    };
    let bytes = ser_to_bytes(&value).unwrap();
    let decoded: Borrowed = de_from_slice(&bytes).unwrap();
    assert_eq!(decoded, value);
    let range = bytes.as_ptr() as usize..bytes.as_ptr() as usize + bytes.len();
    assert!(range.contains(&(decoded.text.as_ptr() as usize)));
    assert!(range.contains(&(decoded.data.as_ptr() as usize)));
}

#[test]
/// Exercises mixed nested Lists, ordered Maps, Options, and default array Tuples.
fn test_nested_collections_and_binary_rust_forms() {
    let value = (
        vec![vec![1i8, 2], vec![]],
        BTreeMap::from([(3u32, Some("value".to_string())), (4, None)]),
        [true, false],
    );
    assert_eq!(
        de_from_slice::<(Vec<Vec<i8>>, BTreeMap<u32, Option<String>>, [bool; 2])>(
            &ser_to_bytes(&value).unwrap()
        )
        .unwrap(),
        value
    );
}

#[test]
/// Checks unsupported Rust forms at runtime, even with handwritten valid schemas.
fn test_unsupported_rust_forms() {
    #[derive(Debug, Serialize, Deserialize)]
    /// Unsupported unit struct, given a scalar schema to reach the Serde check.
    struct Unit;
    #[derive(Debug, Serialize, Deserialize)]
    /// Unsupported newtype struct, not the special internal DateTime marker.
    struct Newtype(
        /// Wrapped scalar used to reach the newtype callback.
        i32,
    );
    #[derive(Debug, Serialize, Deserialize)]
    /// Unsupported tuple struct, which is not an ordinary Rust tuple.
    struct Tuple(
        /// First positional field.
        i32,
        /// Second positional field.
        String,
    );
    impl Schema for Unit {
        /// Supplies a valid scalar so schema validation is not the failing step.
        fn schema(_: &mut asonb::SchemaBuilder) -> Result<Type, asonb::AsonbError> {
            Ok(Type::I32)
        }
    }
    impl Schema for Newtype {
        /// Describes the scalar payload while leaving the newtype unsupported.
        fn schema(_: &mut asonb::SchemaBuilder) -> Result<Type, asonb::AsonbError> {
            Ok(Type::I32)
        }
    }
    impl Schema for Tuple {
        /// Supplies valid positional types to isolate tuple-struct rejection.
        fn schema(builder: &mut asonb::SchemaBuilder) -> Result<Type, asonb::AsonbError> {
            <(i32, String)>::schema(builder)
        }
    }
    assert!(ser_to_bytes(&()).is_err());
    assert!(
        ser_to_bytes(&Unit)
            .unwrap_err()
            .to_string()
            .contains("Unit structs")
    );
    assert!(
        ser_to_bytes(&Newtype(1))
            .unwrap_err()
            .to_string()
            .contains("Newtype structs")
    );
    assert!(
        ser_to_bytes(&Tuple(1, "a".into()))
            .unwrap_err()
            .to_string()
            .contains("Tuple structs")
    );
    let scalar = ser_to_bytes(&1i32).unwrap();
    let tuple = ser_to_bytes(&(1i32, "a")).unwrap();
    for error in [
        de_from_slice::<()>(&scalar).unwrap_err(),
        de_from_slice::<Unit>(&scalar).unwrap_err(),
        de_from_slice::<Newtype>(&scalar).unwrap_err(),
        de_from_slice::<Tuple>(&tuple).unwrap_err(),
        de_from_reader::<(), _>(scalar.as_slice()).unwrap_err(),
        de_from_reader::<Unit, _>(scalar.as_slice()).unwrap_err(),
        de_from_reader::<Newtype, _>(scalar.as_slice()).unwrap_err(),
        de_from_reader::<Tuple, _>(tuple.as_slice()).unwrap_err(),
    ] {
        assert!(error.to_string().contains("unsupported"));
    }
    #[derive(Serialize, Schema)]
    /// An Object with no fields fails schema arity validation.
    struct Empty {}
    #[derive(Serialize, Schema)]
    /// Becomes an invalid empty Object after its only field is skipped.
    struct Skipped {
        #[serde(skip)]
        /// Present in Rust but absent from the document schema.
        _value: i32,
    }
    #[derive(Serialize, Schema)]
    /// Has a struct-like branch whose payload is illegally empty.
    enum EmptyVariant {
        /// Named syntax does not make an empty payload a unit variant.
        Empty {},
    }
    assert!(ser_to_bytes(&Empty {}).is_err());
    assert!(ser_to_bytes(&Skipped { _value: 1 }).is_err());
    assert!(ser_to_bytes(&EmptyVariant::Empty {}).is_err());
    assert!(ser_to_bytes(&[0i32; 0]).is_err());
    assert!(ser_to_bytes(&None::<()>).is_err());
    let one = (42i32,);
    assert_eq!(
        de_from_slice::<(i32,)>(&ser_to_bytes(&one).unwrap()).unwrap(),
        one
    );
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    /// A newtype variant may validly contain a one-element Tuple as its one value.
    enum SingleTuple {
        /// Distinguishes one tuple-valued payload from a tuple-like variant.
        Value(
            /// The sole payload is itself a nonempty Tuple.
            (i32,),
        ),
    }
    let value = SingleTuple::Value(one);
    assert_eq!(
        de_from_slice::<SingleTuple>(&ser_to_bytes(&value).unwrap()).unwrap(),
        value
    );
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
    let bytes = ser_to_bytes(&vec![vec![1i32, 2], vec![3, 4]]).unwrap();
    assert_eq!(
        de_from_slice::<Vec<Vec<i32>>>(&bytes).unwrap(),
        vec![vec![1, 2], vec![3, 4]]
    );
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
    let values = vec!["payload".to_string(); 16];
    let mut bytes = Vec::new();
    asonb::ser_to_writer(&values, &mut bytes).unwrap();
    assert_eq!(de_from_slice::<Vec<String>>(&bytes).unwrap(), values);
    assert_eq!(
        asonb::de_from_reader::<Vec<String>, _>(bytes.as_slice()).unwrap(),
        values
    );
    let map = BTreeMap::from([(1u8, "first".to_string()), (2, "second".to_string())]);
    let mut bytes = Vec::new();
    asonb::ser_to_writer(&map, &mut bytes).unwrap();
    assert_eq!(de_from_slice::<BTreeMap<u8, String>>(&bytes).unwrap(), map);
}

#[test]
/// Checks interoperability between streamed chunks and materialized dynamic Lists.
fn test_stream_values_round_trip() {
    let values = vec!["payload".to_string(); 16];
    let mut bytes = Vec::new();
    {
        let mut writer = asonb::list_stream_writer::<String, _>(&mut bytes);
        writer.start_list().unwrap();
        for value in &values {
            writer.write(value).unwrap();
        }
        writer.end_list().unwrap();
    }
    let decoded = asonb::list_stream_reader::<String, _>(bytes.as_slice())
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(decoded, values);
    assert_eq!(de_from_slice::<Vec<String>>(&bytes).unwrap(), values);
    let schema = DocumentSchema::new(Type::List(
        Encoding::MultiChunk,
        Box::new(Type::String(Encoding::Regular)),
    ));
    let value = Value::List(
        values
            .iter()
            .map(|text| Value::String(Cow::Borrowed(text.as_str())))
            .collect(),
    );
    let mut bytes = Vec::new();
    ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
    assert_eq!(
        asonb::list_stream_reader::<String, _>(bytes.as_slice())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        values
    );
}

#[test]
/// Keeps deep-but-acyclic schemas valid without an artificial nesting quota.
fn test_unrestricted_schema_round_trip() {
    let name_schema = DocumentSchema::new(Type::Object(
        "Record".into(),
        vec![Field::new("value", Type::I32)],
    ));
    let value = Value::Object(vec![Value::I32(1)]);
    let mut bytes = Vec::new();
    ser_value_to_writer(&name_schema, &value, &mut bytes).unwrap();
    assert_eq!(
        asonb::de_value_from_slice(&bytes).unwrap(),
        (name_schema.clone(), value.clone())
    );
    assert_eq!(
        asonb::de_value_from_reader(bytes.as_slice()).unwrap(),
        (name_schema, value)
    );
    let tuple_schema = DocumentSchema::new(Type::Tuple(vec![Type::I32; 3]));
    tuple_schema.validate().unwrap();
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
    let time = asonb::date_time::DateTime(
        chrono::DateTime::from_timestamp(0, 0)
            .unwrap()
            .with_timezone(&chrono::FixedOffset::east_opt(0).unwrap()),
    );
    let mut bytes = Vec::new();
    asonb::ser_to_writer(&time, &mut bytes).unwrap();
    assert_eq!(
        de_from_slice::<asonb::date_time::DateTime>(&bytes).unwrap(),
        time
    );
}

#[test]
/// Rejects recursive type definitions and malformed data even in ignored fields.
fn test_recursive_schema_and_ignored_field_validation() {
    #[derive(Serialize, Schema)]
    /// The type is recursive even when a particular instance has next = None.
    struct Recursive {
        /// Following this child would require an infinite static type expansion.
        next: Option<Box<Recursive>>,
    }
    assert!(DocumentSchema::of::<Recursive>().is_err());
    let recursive = Recursive { next: None };
    let mut output = Vec::new();
    assert!(asonb::ser_to_writer(&recursive, &mut output).is_err());
    assert!(output.is_empty());
    let mut stream = asonb::list_stream_writer::<Recursive, _>(&mut output);
    assert!(stream.start_list().is_err());
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    /// Omits the writer's text field so Serde exercises validated skipping.
    struct Target {
        /// The only field the target chooses to retain.
        id: i32,
    }
    let schema = DocumentSchema::new(Type::Object(
        "".into(),
        vec![
            Field::new("id", Type::I32),
            Field::new("ignored", Type::String(Encoding::Regular)),
        ],
    ));
    let mut bytes = Vec::new();
    ser_value_to_writer(
        &schema,
        &Value::Object(vec![Value::I32(11), Value::String("a".into())]),
        &mut bytes,
    )
    .unwrap();
    assert!(de_from_slice::<Target>(&bytes).is_ok());
    // Corrupt only the ignored String's UTF-8. Ignoring its value must not make
    // malformed bytes acceptable to a schema-aware reader.
    *bytes.last_mut().unwrap() = 0xff;
    assert!(de_from_slice::<Target>(&bytes).is_err());
}

#[test]
/// Checks span-aware compiler diagnostics for unsupported derive inputs.
fn test_derive_compile_errors() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/*.rs");
}

proptest::proptest! {
    #[test]
    /// Generates mixed scalar/text payloads to check typed List round trips.
    fn test_generated_round_trips(values in proptest::collection::vec((proptest::prelude::any::<i64>(),proptest::prelude::any::<bool>(),".*"),0..30)){
        let bytes=ser_to_bytes(&values).unwrap();proptest::prop_assert_eq!(de_from_slice::<Vec<(i64,bool,String)>>(&bytes).unwrap(),values);
    }
    #[test]
    /// Allows decoding errors for arbitrary bytes, but never a decoder panic.
    fn test_arbitrary_inputs_never_panic(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(),0..256)){
        let _=asonb::de_value_from_slice(&bytes);let _=de_from_slice::<Vec<String>>(&bytes);
    }
}
