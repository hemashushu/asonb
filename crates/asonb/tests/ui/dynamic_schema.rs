// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

#[derive(serde::Serialize, asonb::Schema)]
struct Dynamic {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<i32>,
}
fn main() {}