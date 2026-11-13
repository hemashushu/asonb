// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

#[derive(asonb::Schema)]
#[asonb(unknown = "value")]
struct UnknownAttribute {
    value: i32,
}

#[derive(asonb::Schema)]
#[serde(rename_all = "unknown_case")]
struct InvalidRenameRule {
    value: i32,
}

#[derive(asonb::Schema)]
struct InvalidSchemaPath {
    #[asonb(schema_with = "not a path")]
    value: i32,
}

fn main() {}
