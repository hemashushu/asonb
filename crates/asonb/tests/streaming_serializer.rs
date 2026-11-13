// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Tests for interoperability between streamed and materialized Lists.

use asonb::{DocumentSchema, Encoding, Type, Value, de_from_slice, ser_value_to_writer};
use pretty_assertions::assert_eq;
use std::borrow::Cow;

#[test]
fn test_list_streams_distinguish_boundary_eof_from_partial_chunk_headers() {
    let mut bytes = Vec::new();
    let mut writer = asonb::list_stream_writer::<i32, _>(&mut bytes);
    writer.start_list().unwrap();
    writer.writes(&[11, 13]).unwrap();
    writer.end_list().unwrap();
    bytes.truncate(bytes.len() - 4);

    assert_eq!(
        asonb::list_stream_reader::<i32, _>(bytes.as_slice())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        [11, 13]
    );
    assert_eq!(
        asonb::value_document_reader(bytes.as_slice())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        [Value::I32(11), Value::I32(13)]
    );

    for prefix_length in 1..4 {
        let mut partial = bytes.clone();
        partial.extend_from_slice(&[0; 3][..prefix_length]);
        let mut reader = asonb::list_stream_reader::<i32, _>(partial.as_slice()).unwrap();
        assert_eq!(reader.next().unwrap().unwrap(), 11);
        assert_eq!(reader.next().unwrap().unwrap(), 13);
        assert!(reader.next().unwrap().is_err());
        assert!(reader.next().is_none());
        assert!(reader.next().is_none());

        let mut reader = asonb::value_document_reader(partial.as_slice()).unwrap();
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(11));
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(13));
        assert!(reader.next().unwrap().is_err());
        assert!(reader.next().is_none());
        assert!(reader.next().is_none());
    }
}

#[test]
fn test_nonstreaming_roots_are_materialized_once() {
    for (bytes, expected) in [
        (asonb::ser_to_bytes(&11i32).unwrap(), Value::I32(11)),
        (
            asonb::ser_to_bytes(&vec![11i32, 13]).unwrap(),
            Value::List(vec![Value::I32(11), Value::I32(13)]),
        ),
    ] {
        let error = match asonb::list_stream_reader::<i32, _>(bytes.as_slice()) {
            Ok(_) => panic!("expected a non-streaming root to be rejected"),
            Err(error) => error,
        };
        assert!(
            error.to_string().contains("multi-chunk root List"),
            "{error}"
        );

        let mut reader = asonb::value_document_reader(bytes.as_slice()).unwrap();
        assert!(!reader.is_streaming_list());
        assert!(!reader.is_streaming_result());
        assert_eq!(reader.next().unwrap().unwrap(), expected);
        assert!(reader.next().is_none());
        assert!(reader.next().is_none());
    }
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
