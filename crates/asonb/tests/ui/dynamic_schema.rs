// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Compile-fail fixture: conditional field omission has no value-independent schema.

#[derive(serde::Serialize, asonb::Schema)]
/// Would change its field count depending on whether value is None.
struct Dynamic {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Cannot be assigned one fixed Object position for all serialized values.
    value: Option<i32>,
}

#[derive(asonb::Schema)]
#[serde(untagged)]
enum Untagged {
    Integer(i32),
    Text(String),
}

#[derive(asonb::Schema)]
#[serde(tag = "kind")]
enum InternallyTagged {
    Ready,
}

#[derive(asonb::Schema)]
#[serde(content = "payload")]
enum ContentTagged {
    Value(i32),
}

/// No runtime behavior is needed; deriving this type must fail during compilation.
fn main() {}
