// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Reads the binary List produced by the stream-writer example from stdin.
//! Run the two examples as a pipe. The reader needs Deserialize but not Schema:
//! type metadata arrives in the document itself.

use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
/// The owned record expected from each root List element.
struct Object {
    /// Numeric identifier carried in the document's i32 field.
    id: i32,
    /// Owned text because a stdin reader cannot lend stable input storage.
    name: String,
}

/// Collects this small demonstration stream and checks both expected records.
/// A real consumer can process each iterator item immediately instead of
/// collecting the complete result in memory. Errors propagate to the process.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::io::stdin().lock();
    // collect stops at the explicit List marker, without waiting for stdin EOF.
    let values = asonb::list_stream_reader::<Object, _>(input)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        values,
        vec![
            Object {
                id: 11,
                name: "foo".to_owned()
            },
            Object {
                id: 13,
                name: "bar".to_owned()
            }
        ]
    );
    // Diagnostics stay on stderr so stdout remains usable in a larger pipeline.
    eprintln!("Streaming deserialization successful!");
    Ok(())
}
