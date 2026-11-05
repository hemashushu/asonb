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

/// Writes a fixed-size array as a Serde sequence, which ASONB encodes as a List.
///
/// `value` is the array to write; its elements stay in their original order.
/// `serializer` is the Serde destination supplied by the containing value.
/// Errors from that destination are returned unchanged.
///
/// Use this through `#[serde(with = "asonb::array")]`, not by calling it yourself:
///
/// ```
/// #[derive(serde::Serialize, serde::Deserialize, asonb::Schema)]
/// struct Samples {
///     #[serde(with = "asonb::array")]
///     readings: [i32; 3],
/// }
/// let bytes = asonb::ser_to_bytes(&Samples { readings: [2, 4, 6] })?;
/// let decoded: Samples = asonb::de_from_slice(&bytes)?;
/// assert_eq!(decoded.readings, [2, 4, 6]);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn serialize<T: Serialize, S: Serializer, const N: usize>(
    value: &[T; N],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    // A slice asks Serde for a sequence; an array normally asks for a tuple.
    value.as_slice().serialize(serializer)
}

/// Reads a Serde sequence and requires exactly `N` elements.
///
/// `deserializer` supplies the sequence, and `T` decides how each element is
/// decoded. The elements are collected before the length is checked, so an
/// oversized input is not rejected early. Empty arrays are supported.
/// A wrong length or an invalid element returns the deserializer's error type.
///
/// The `#[serde(with = "asonb::array")]` example in [`serialize`] uses both
/// adapter functions, keeping the Serde mapping and derived schema in sync:
///
/// ```
/// #[derive(serde::Serialize, serde::Deserialize, asonb::Schema)]
/// struct Samples {
///     #[serde(with = "asonb::array")]
///     readings: [i32; 3],
/// }
/// let bytes = asonb::ser_to_bytes(&Samples { readings: [2, 4, 6] })?;
/// let decoded: Samples = asonb::de_from_slice(&bytes)?;
/// assert_eq!(decoded.readings, [2, 4, 6]);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>, const N: usize>(
    deserializer: D,
) -> Result<[T; N], D::Error> {
    let values = Vec::<T>::deserialize(deserializer)?;
    let actual = values.len();
    // Vec-to-array conversion moves the elements without requiring T: Copy.
    values
        .try_into()
        .map_err(|_| serde::de::Error::custom(format!("expected {N} elements, found {actual}")))
}
