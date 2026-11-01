// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Explicitly map fixed-size Rust arrays to ASONB Lists.
//!
//! Use `#[serde(with = "asonb::array")]` on a `[T; N]` field. The adapter
//! supports any array length, including zero, and deserialization requires
//! exactly `N` List elements.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<T: Serialize, S: Serializer, const N: usize>(
    value: &[T; N],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value.as_slice().serialize(serializer)
}

pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>, const N: usize>(
    deserializer: D,
) -> Result<[T; N], D::Error> {
    let values = Vec::<T>::deserialize(deserializer)?;
    let actual = values.len();
    values
        .try_into()
        .map_err(|_| serde::de::Error::custom(format!("expected {N} elements, found {actual}")))
}
