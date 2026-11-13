// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

#[derive(asonb::Schema)]
struct MissingSchemaOverride {
    #[serde(serialize_with = "custom_serializer")]
    value: i32,
}

#[derive(asonb::Schema)]
struct NotAnArray {
    #[serde(with = "asonb::array")]
    values: Vec<i32>,
}

fn main() {}
