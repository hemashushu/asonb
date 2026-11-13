// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! End-to-end tests for Serde serialization, deserialization, and schema derivation.

use asonb::{
    DocumentSchema, Encoding, Field, Schema, Type, Value, Variant, VariantValue, de_from_reader,
    de_from_slice, ser_to_bytes, ser_value_to_writer,
};
use pretty_assertions::assert_eq;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::{self, Debug},
};

#[track_caller]
fn assert_serde_round_trip<T>(value: &T) -> Vec<u8>
where
    T: Serialize + DeserializeOwned + Schema + Debug + PartialEq,
{
    let bytes = ser_to_bytes(value).unwrap();
    let mut output = Vec::new();
    asonb::ser_to_writer(value, &mut output).unwrap();
    assert_eq!(output, bytes);
    assert_eq!(&de_from_slice::<T>(&bytes).unwrap(), value);
    assert_eq!(&de_from_reader::<T, _>(bytes.as_slice()).unwrap(), value);
    bytes
}

#[track_caller]
fn assert_read_error<T: DeserializeOwned + Debug>(bytes: &[u8], expected: &str) {
    for error in [
        de_from_slice::<T>(bytes).unwrap_err(),
        de_from_reader::<T, _>(bytes).unwrap_err(),
    ] {
        assert!(error.to_string().contains(expected), "{error}");
    }
}

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
    assert_serde_round_trip(&value);
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
    /// Intentionally differs from the writer's field order.
    struct Target {
        /// Appears after id on the wire, but before id in this Rust structure.
        name: String,
        /// Matched by name, not by its Rust field position.
        id: i32,
        #[serde(default)]
        /// Missing on the wire and therefore supplied by Serde's default.
        active: bool,
    }
    let schema = DocumentSchema::new(Type::Object(vec![
        Field::new("id", Type::I32),
        Field::new("extra", Type::List(Encoding::Regular, Box::new(Type::I32))),
        Field::new("name", Type::String(Encoding::Regular)),
    ]));
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
        &Value::Enumeration {
            index: 0,
            payload: VariantValue::Newtype(Box::new(Value::I32(9))),
        },
        &mut bytes,
    )
    .unwrap();
    assert_eq!(de_from_slice::<Event>(&bytes).unwrap(), Event::Number(9));
}

#[test]
/// Derived types cannot redefine Option/Result or use reserved Object keys.
fn test_predefined_enumerations_and_reserved_keys() {
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    /// A user enum reusing the predefined name with an extra variant.
    enum Option {
        /// Matches the predefined unit variant.
        None,
        /// Matches the predefined single-value variant.
        Some(i32),
        /// Not part of the predefined definition.
        Other,
    }
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(rename = "Result")]
    /// Renamed to the predefined name, with a changed payload form.
    enum Outcome {
        /// Must carry exactly one value in the predefined definition.
        Ok,
        /// Matches the predefined single-value variant.
        Err(String),
    }
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    /// An Object whose key is an ASON keyword.
    struct Reserved {
        #[serde(rename = "NaN")]
        /// Renamed to a reserved keyword.
        nan: i32,
    }
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(rename = "Some")]
    /// Variant shorthand names are ordinary identifiers.
    enum Shorthand {
        /// Field named after a variant shorthand.
        None {
            #[serde(rename = "Ok")]
            /// Renamed to a variant shorthand.
            ok: i32,
        },
    }
    assert!(DocumentSchema::of::<Option>().is_err());
    assert!(ser_to_bytes(&Option::None).is_err());
    assert!(ser_to_bytes(&Outcome::Ok).is_err());
    assert!(ser_to_bytes(&Reserved { nan: 1 }).is_err());
    let value = Shorthand::None { ok: 1 };
    assert_eq!(
        de_from_slice::<Shorthand>(&ser_to_bytes(&value).unwrap()).unwrap(),
        value
    );

    // Rust's Option and Result still map to the predefined enumerations, and
    // matching by name keeps either wire variant order decodable.
    let schema = DocumentSchema::new(Type::Enum(
        "Option".into(),
        vec![
            Variant::Newtype("Some".into(), Type::I32),
            Variant::Unit("None".into()),
        ],
    ));
    for (value, expected) in [
        (
            Value::Enumeration {
                index: 0,
                payload: VariantValue::Newtype(Box::new(Value::I32(11))),
            },
            Some(11),
        ),
        (
            Value::Enumeration {
                index: 1,
                payload: VariantValue::Unit,
            },
            None,
        ),
    ] {
        let mut bytes = Vec::new();
        ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
        assert_eq!(
            de_from_slice::<std::option::Option<i32>>(&bytes).unwrap(),
            expected
        );
    }
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
            Type::Object(fields) => Some(fields),
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
        let schema = DocumentSchema::new(Type::Object(vec![Field::new(
            "values",
            Type::List(Encoding::Regular, Box::new(Type::I32)),
        )]));
        let mut bytes = Vec::new();
        ser_value_to_writer(
            &schema,
            &Value::Object(vec![Value::List(values)]),
            &mut bytes,
        )
        .unwrap();
        assert_read_error::<TwoItems>(&bytes, "expected 2 elements");
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
    assert_serde_round_trip(&value);
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
fn test_custom_adapter_schema_override_and_transparent_wrapper() {
    fn flag_schema(_: &mut asonb::SchemaBuilder) -> Result<Type, asonb::AsonbError> {
        Ok(Type::U8)
    }
    fn serialize_flag<S: serde::Serializer>(
        value: &bool,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(u8::from(*value))
    }
    fn deserialize_flag<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<bool, D::Error> {
        Ok(u8::deserialize(deserializer)? != 0)
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(transparent)]
    struct Flag {
        #[serde(
            serialize_with = "serialize_flag",
            deserialize_with = "deserialize_flag"
        )]
        #[asonb(schema_with = "flag_schema")]
        value: bool,
        #[serde(skip)]
        ignored: i32,
    }

    for value in [false, true] {
        let bytes = assert_serde_round_trip(&Flag { value, ignored: 0 });
        assert_eq!(
            asonb::de_value_from_slice(&bytes).unwrap(),
            (DocumentSchema::new(Type::U8), Value::U8(u8::from(value)))
        );
    }

    fn failing_schema(_: &mut asonb::SchemaBuilder) -> Result<Type, asonb::AsonbError> {
        Err(asonb::AsonbError::Message("schema override failed".into()))
    }
    #[derive(Serialize, Schema)]
    struct Invalid {
        #[asonb(schema_with = "failing_schema")]
        value: bool,
    }
    let mut output = Vec::new();
    let error = asonb::ser_to_writer(&Invalid { value: true }, &mut output).unwrap_err();
    assert!(
        error.to_string().contains("schema override failed"),
        "{error}"
    );
    assert!(output.is_empty());
}

#[test]
fn test_variant_skips_and_serialization_names() {
    #[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
    #[serde(
        rename(serialize = "WireEvent", deserialize = "WireEvent"),
        rename_all(serialize = "snake_case", deserialize = "snake_case"),
        rename_all_fields = "camelCase"
    )]
    enum Named {
        Ready,
        #[serde(skip)]
        Hidden,
        UpdateRecord {
            request_id: u32,
            r#type: String,
        },
        #[serde(rename_all = "UPPERCASE")]
        OtherRecord {
            field_name: bool,
        },
    }

    for (value, index, fields) in [
        (
            Named::UpdateRecord {
                request_id: 7,
                r#type: "update".into(),
            },
            1,
            vec!["requestId", "type"],
        ),
        (
            Named::OtherRecord { field_name: true },
            2,
            vec!["FIELD_NAME"],
        ),
    ] {
        let bytes = assert_serde_round_trip(&value);
        let (schema, decoded) = asonb::de_value_from_slice(&bytes).unwrap();
        let variants = schema
            .references
            .iter()
            .find_map(|ty| match ty {
                Type::Enum(name, variants) if name == "WireEvent" => Some(variants),
                _ => None,
            })
            .unwrap();
        assert_eq!(variants.len(), 3);
        let Variant::Struct(_, actual_fields) = &variants[index] else {
            panic!("expected a struct variant");
        };
        assert_eq!(
            actual_fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            fields
        );
        assert!(
            matches!(decoded, Value::Enumeration { index: actual, .. } if actual as usize == index)
        );
    }
    assert!(ser_to_bytes(&Named::Hidden).is_err());
}

#[test]
fn test_custom_map_serializer_validates_pair_protocol() {
    use serde::ser::SerializeMap;

    enum Operation {
        ValueWithoutKey,
        ConsecutiveKeys,
        MissingValue,
        TooManyPairs,
        TooFewPairs,
        UnknownLength,
    }
    struct Producer(Operation);
    impl Schema for Producer {
        fn schema(builder: &mut asonb::SchemaBuilder) -> Result<Type, asonb::AsonbError> {
            BTreeMap::<i32, i32>::schema(builder)
        }
    }
    impl Serialize for Producer {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let length = if matches!(self.0, Operation::UnknownLength) {
                None
            } else {
                Some(1)
            };
            let mut map = serializer.serialize_map(length)?;
            match self.0 {
                Operation::ValueWithoutKey => map.serialize_value(&2i32)?,
                Operation::ConsecutiveKeys => {
                    map.serialize_key(&1i32)?;
                    map.serialize_key(&2i32)?;
                }
                Operation::MissingValue => map.serialize_key(&1i32)?,
                Operation::TooManyPairs => {
                    map.serialize_entry(&1i32, &2i32)?;
                    map.serialize_entry(&3i32, &4i32)?;
                }
                Operation::TooFewPairs => {}
                Operation::UnknownLength => map.serialize_entry(&1i32, &2i32)?,
            }
            map.end()
        }
    }

    for (operation, expected) in [
        (Operation::ValueWithoutKey, "Map key is missing"),
        (Operation::ConsecutiveKeys, "Map value is missing"),
        (Operation::MissingValue, "Incomplete container"),
        (Operation::TooManyPairs, "Too many Map pairs"),
        (Operation::TooFewPairs, "Incomplete container"),
    ] {
        let error = ser_to_bytes(&Producer(operation)).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
    let bytes = ser_to_bytes(&Producer(Operation::UnknownLength)).unwrap();
    assert_eq!(
        de_from_slice::<BTreeMap<i32, i32>>(&bytes).unwrap(),
        BTreeMap::from([(1, 2)])
    );
}

#[test]
fn test_custom_visitors_must_consume_complete_containers() {
    #[derive(Debug)]
    struct FirstElement;
    impl<'de> serde::de::Visitor<'de> for FirstElement {
        type Value = Self;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a list")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut access: A) -> Result<Self, A::Error> {
            let _: Option<i32> = access.next_element()?;
            Ok(Self)
        }
    }
    impl<'de> Deserialize<'de> for FirstElement {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_seq(Self)
        }
    }

    #[derive(Debug)]
    struct BadMap<const CASE: u8>;
    impl<'de, const CASE: u8> serde::de::Visitor<'de> for BadMap<CASE> {
        type Value = Self;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut access: A) -> Result<Self, A::Error> {
            match CASE {
                0 => {
                    let _: i32 = access.next_value()?;
                }
                1 => {
                    let _: Option<i32> = access.next_key()?;
                    let _: Option<i32> = access.next_key()?;
                }
                2 => {
                    let _: Option<(i32, i32)> = access.next_entry()?;
                }
                _ => {
                    let _: Option<i32> = access.next_key()?;
                }
            }
            Ok(Self)
        }
    }
    impl<'de, const CASE: u8> Deserialize<'de> for BadMap<CASE> {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_map(Self)
        }
    }

    let bytes = ser_to_bytes(&vec![1i32, 2]).unwrap();
    assert_read_error::<FirstElement>(&bytes, "complete sequence");
    let bytes = ser_to_bytes(&BTreeMap::from([(1i32, 2i32), (3, 4)])).unwrap();
    assert_read_error::<BadMap<0>>(&bytes, "Map key was not consumed");
    assert_read_error::<BadMap<1>>(&bytes, "Map value was not consumed");
    assert_read_error::<BadMap<2>>(&bytes, "complete map");
    assert_read_error::<BadMap<3>>(&bytes, "complete map");
}

#[test]
fn test_custom_sequence_serializer_enforces_element_counts() {
    use serde::ser::SerializeSeq;

    struct Producer {
        promised: Option<usize>,
        actual: usize,
    }
    impl Schema for Producer {
        fn schema(builder: &mut asonb::SchemaBuilder) -> Result<Type, asonb::AsonbError> {
            Vec::<i32>::schema(builder)
        }
    }
    impl Serialize for Producer {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut sequence = serializer.serialize_seq(self.promised)?;
            for value in 0..self.actual {
                sequence.serialize_element(&(value as i32))?;
            }
            sequence.end()
        }
    }

    for (promised, actual, message) in [(0, 1, "Too many elements"), (2, 1, "Incomplete container")]
    {
        let error = ser_to_bytes(&Producer {
            promised: Some(promised),
            actual,
        })
        .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
    }
    for promised in [None, Some(3)] {
        let bytes = ser_to_bytes(&Producer {
            promised,
            actual: 3,
        })
        .unwrap();
        assert_eq!(de_from_slice::<Vec<i32>>(&bytes).unwrap(), [0, 1, 2]);
    }
}

#[test]
fn test_enum_deserialization_rejects_mismatched_payload_shapes() {
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    enum Target {
        Unit,
        Newtype(i32),
        Tuple(i32, i32),
        Struct { id: i32 },
    }

    for (variant, payload, expected) in [
        (
            Variant::Newtype("Unit".into(), Type::I32),
            VariantValue::Newtype(Box::new(Value::I32(1))),
            "without a payload",
        ),
        (
            Variant::Unit("Newtype".into()),
            VariantValue::Unit,
            "single-value variant",
        ),
        (
            Variant::Newtype("Tuple".into(), Type::I32),
            VariantValue::Newtype(Box::new(Value::I32(1))),
            "tuple-like variant",
        ),
        (
            Variant::Tuple("Struct".into(), vec![Type::I32, Type::I32]),
            VariantValue::Tuple(vec![Value::I32(1), Value::I32(2)]),
            "object-like variant",
        ),
        (
            Variant::Unit("Unknown".into()),
            VariantValue::Unit,
            "unknown variant",
        ),
    ] {
        let schema = DocumentSchema::new(Type::Enum("Target".into(), vec![variant]));
        let mut bytes = Vec::new();
        ser_value_to_writer(
            &schema,
            &Value::Enumeration { index: 0, payload },
            &mut bytes,
        )
        .unwrap();
        assert_read_error::<Target>(&bytes, expected);
    }

    let mut bytes = ser_to_bytes(&Event::None).unwrap();
    let end = bytes.len();
    bytes[end - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_read_error::<Target>(&bytes, "Variant index out of range");
}

#[test]
fn test_public_readers_and_writers_propagate_io_failures() {
    use std::io::{self, Cursor, Read, Write};

    struct ShortReader<'a> {
        bytes: &'a [u8],
        interrupt: bool,
        fail_at_end: bool,
    }
    impl Read for ShortReader<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            if output.is_empty() {
                return Ok(0);
            }
            self.interrupt = !self.interrupt;
            if self.interrupt {
                return Err(io::ErrorKind::Interrupted.into());
            }
            if self.bytes.is_empty() && self.fail_at_end {
                return Err(io::Error::other("test read failure"));
            }
            self.bytes.read(&mut output[..1])
        }
    }
    struct ShortWriter {
        bytes: Vec<u8>,
        interrupt: bool,
    }
    impl Write for ShortWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.is_empty() {
                return Ok(0);
            }
            self.interrupt = !self.interrupt;
            if self.interrupt {
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.bytes.push(bytes[0]);
            Ok(1)
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("serialization must not flush the writer");
        }
    }

    let value = vec!["first".to_owned(), "second".to_owned()];
    let bytes = ser_to_bytes(&value).unwrap();
    let (schema, dynamic) = asonb::de_value_from_slice(&bytes).unwrap();
    let reader = || ShortReader {
        bytes: &bytes,
        interrupt: false,
        fail_at_end: false,
    };
    assert_eq!(de_from_reader::<Vec<String>, _>(reader()).unwrap(), value);
    assert_eq!(
        asonb::de_value_from_reader(reader()).unwrap(),
        (schema.clone(), dynamic.clone())
    );
    let mut writer = ShortWriter {
        bytes: Vec::new(),
        interrupt: false,
    };
    asonb::ser_to_writer(&value, &mut writer).unwrap();
    assert_eq!(writer.bytes, bytes);

    for end in 0..=bytes.len() {
        let reader = || ShortReader {
            bytes: &bytes[..end],
            interrupt: false,
            fail_at_end: true,
        };
        for error in [
            de_from_reader::<Vec<String>, _>(reader()).unwrap_err(),
            asonb::de_value_from_reader(reader()).unwrap_err(),
        ] {
            assert!(
                error.to_string().contains("test read failure"),
                "at {end}: {error}"
            );
        }

        let mut typed_output = vec![0; end];
        let mut dynamic_output = vec![0; end];
        let typed_result = asonb::ser_to_writer(&value, &mut Cursor::new(&mut typed_output[..]));
        let dynamic_result =
            ser_value_to_writer(&schema, &dynamic, &mut Cursor::new(&mut dynamic_output[..]));
        assert_eq!(typed_result.is_ok(), end == bytes.len());
        assert_eq!(dynamic_result.is_ok(), end == bytes.len());
        assert_eq!(typed_output, bytes[..end]);
        assert_eq!(dynamic_output, bytes[..end]);
    }
}

proptest::proptest! {
    #[test]
    fn test_generated_round_trips(
        values in proptest::collection::vec(
            (proptest::prelude::any::<i64>(), proptest::prelude::any::<bool>(), ".*"),
            0..30,
        ),
    ) {
        assert_serde_round_trip(&values);
    }
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
    let schema = DocumentSchema::new(Type::Object(vec![
        Field::new("id", Type::I32),
        Field::new("ignored", Type::String(Encoding::Regular)),
    ]));
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
/// Skips every container form through `IgnoredAny` while still rejecting truncated payloads.
fn test_ignored_any_validates_every_container_kind() {
    #[derive(Serialize, Schema)]
    enum Event {
        Idle,
        Data(i64),
        Move(i32, i32),
        Named { id: u8, label: String },
    }
    #[derive(Serialize, Schema)]
    struct Record {
        id: u32,
        tags: BTreeMap<String, Vec<i32>>,
        pair: (bool, String),
        events: Vec<Event>,
    }
    let record = Record {
        id: 7,
        tags: BTreeMap::from([("a".into(), vec![1, 2]), ("b".into(), vec![])]),
        pair: (true, "x".into()),
        events: vec![
            Event::Idle,
            Event::Data(-1),
            Event::Move(3, 4),
            Event::Named {
                id: 9,
                label: "named".into(),
            },
        ],
    };
    let bytes = ser_to_bytes(&record).unwrap();
    assert!(de_from_slice::<serde::de::IgnoredAny>(&bytes).is_ok());
    assert!(de_from_reader::<serde::de::IgnoredAny, _>(bytes.as_slice()).is_ok());
    for end in 0..bytes.len() {
        assert!(
            de_from_slice::<serde::de::IgnoredAny>(&bytes[..end]).is_err(),
            "end {end}"
        );
    }

    let schema = DocumentSchema::new(Type::Enum(
        "Event".into(),
        vec![Variant::Unit("Idle".into())],
    ));
    let mut bytes = Vec::new();
    ser_value_to_writer(
        &schema,
        &Value::Enumeration {
            index: 0,
            payload: VariantValue::Unit,
        },
        &mut bytes,
    )
    .unwrap();
    let end = bytes.len();
    bytes[end - 4..].copy_from_slice(&1u32.to_le_bytes());
    assert_read_error::<serde::de::IgnoredAny>(&bytes, "Variant index out of range");
}

#[test]
/// Reports schema mismatches for Serde targets that request a different wire form.
fn test_deserialize_rejects_mismatched_targets() {
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    #[allow(dead_code)]
    enum Untagged {
        Integer(i32),
    }
    let bytes = ser_to_bytes(&11i32).unwrap();
    assert_read_error::<String>(&bytes, "Expected a String");
    assert_read_error::<serde_bytes::ByteBuf>(&bytes, "Expected Byte Data");
    assert_read_error::<Option<i32>>(&bytes, "Expected an Enumeration");
    assert_read_error::<Option<i32>>(
        &ser_to_bytes(&Ok::<i32, String>(1)).unwrap(),
        "Expected None or Some",
    );
    assert_read_error::<asonb::date_time::DateTime>(&bytes, "Expected DateTime");
    assert_read_error::<BTreeMap<i32, i32>>(&bytes, "Expected a matching Object or Map");
    assert_read_error::<Untagged>(&bytes, "deserialize_any is unsupported");
}
