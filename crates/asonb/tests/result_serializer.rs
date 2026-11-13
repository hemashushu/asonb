// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Public API tests for streamed `Result` values.

use asonb::{ResultStreamReader, de_from_slice, result_stream_reader, result_stream_writer};
use serde::{Deserialize, Serialize};

#[test]
fn test_result_ok_stream_round_trip() {
    for values in [vec![], vec![11, 13, 17]] {
        let expected = Ok::<_, String>(values.clone());
        let mut bytes = Vec::new();
        let mut stream = result_stream_writer::<i32, String, _>(&mut bytes)
            .start_ok()
            .unwrap();
        stream.writes(&values).unwrap();
        stream.end_list().unwrap();

        assert_eq!(
            de_from_slice::<Result<Vec<i32>, String>>(&bytes).unwrap(),
            expected
        );
        let ResultStreamReader::Ok(reader) =
            result_stream_reader::<i32, String, _>(bytes.as_slice()).unwrap()
        else {
            panic!("expected a streamed List");
        };
        assert_eq!(reader.collect::<Result<Vec<_>, _>>().unwrap(), values);
    }
}

#[test]
fn test_business_error_stream_preserves_following_document() {
    #[derive(Debug, PartialEq, Serialize, Deserialize, asonb::Schema)]
    struct BusinessError {
        code: u32,
        details: Vec<String>,
    }
    let error = BusinessError {
        code: 7,
        details: vec!["failed".into(), "retry later".into()],
    };
    let mut bytes = Vec::new();
    result_stream_writer::<i32, BusinessError, _>(&mut bytes)
        .write_err(&error)
        .unwrap();

    assert_eq!(
        de_from_slice::<Result<Vec<i32>, BusinessError>>(&bytes).unwrap(),
        Err(error)
    );
    let mut reader = asonb::value_document_reader(bytes.as_slice()).unwrap();
    assert!(!reader.is_streaming_result());
    assert!(matches!(
        reader.next().unwrap().unwrap(),
        asonb::Value::Enumeration {
            payload: asonb::VariantValue::Newtype(_),
            ..
        }
    ));
    assert!(reader.next().is_none());

    let following = asonb::ser_to_bytes(&42i32).unwrap();
    bytes.extend_from_slice(&following);
    let mut source = bytes.as_slice();
    let ResultStreamReader::Err(error) =
        result_stream_reader::<i32, BusinessError, _>(&mut source).unwrap()
    else {
        panic!("expected a business error");
    };
    assert_eq!(error.code, 7);
    assert_eq!(error.details, ["failed", "retry later"]);
    assert_eq!(source, following);
    assert_eq!(asonb::de_from_reader::<i32, _>(source).unwrap(), 42);
    assert!(asonb::value_document_reader(bytes.as_slice()).is_err());
}
