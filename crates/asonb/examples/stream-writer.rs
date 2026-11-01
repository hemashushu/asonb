// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use asonb::Schema;
use serde::Serialize;
use std::io::Write;

#[derive(Serialize, Schema)]
struct Object {
    id: i32,
    name: String,
}

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
    writer.end_list()?;
    output.flush()?;
    eprintln!("Streaming serialization successful!");
    Ok(())
}
