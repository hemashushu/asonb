// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Compile-fail fixture: flattening a Map makes Object fields depend on its keys.

#[derive(serde::Serialize, asonb::Schema)]
/// Tries to merge runtime Map entries into a statically described Object.
struct Flatten {
    #[serde(flatten)]
    /// Arbitrary keys cannot become a fixed, ordered schema field list.
    fields: std::collections::BTreeMap<String, i32>,
}
/// The test checks macro diagnostics; the program is never run.
fn main() {}