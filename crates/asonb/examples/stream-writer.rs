// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Writes a MultiChunk root List to stdout for the stream-reader example.
//! Only document bytes go to stdout; status messages go to stderr so a shell
//! pipe carries an unmodified ASONB document.

use asonb::Schema;
use serde::Serialize;
use std::io::Write;

#[derive(Serialize, Schema)]
/// A record whose schema can be emitted before either example value is written.
struct Object {
    /// A signed identifier, encoded with the i32 wire type.
    id: i32,
    /// A regular UTF-8 String payload.
    name: String,
}

/// Opens the List, emits two independent chunks, and explicitly ends and flushes it.
/// Any schema or I/O error reaches the process rather than being ignored.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = std::io::stdout().lock();
    let mut writer = asonb::list_stream_writer::<Object, _>(&mut output);
    writer.start_list()?;
    writer.write(&Object {
        id: 11,
        name: "foo".to_owned(),
    })?;
    writer.write(&Object {
        id: 13,
        name: "bar".to_owned(),
    })?;
    // Drop alone does not write a marker, and end_list does not flush stdout.
    // Do both explicitly so a waiting consumer sees the complete document.
    writer.end_list()?;
    output.flush()?;
    eprintln!("Streaming serialization successful!");
    Ok(())
}
