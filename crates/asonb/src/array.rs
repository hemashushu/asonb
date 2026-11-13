// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Explicitly map fixed-size Rust arrays to ASONB Lists.
//!
//! Use `#[serde(with = "asonb::array")]` on a `[T; N]` field. The adapter
//! supports any array length, including zero, and deserialization requires
//! exactly `N` List elements. This differs from Serde's default array Tuple
//! representation, whose arity is stored in the schema rather than the payload.
//!
//! The [`derive@crate::Schema`] macro recognizes this adapter path and describes
//! a Regular List of the array's element type. A handwritten [`crate::Schema`]
//! implementation must make the same choice. The adapter does not store `N`
//! in the schema; readers validate it against the actual number of elements.
//! Reading accepts Regular, Long, and MultiChunk List framing.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Writes a fixed-size array as a Serde sequence, which ASONB encodes as a List.
///
/// `value` is the array to write; its elements stay in their original order.
/// `serializer` is the Serde destination supplied by the containing value.
/// The returned `S::Ok` is the serializer's completion value. No `Copy` or
/// `Clone` bound is needed, and an empty array is written as an empty sequence.
///
/// Use this through `#[serde(with = "asonb::array")]`, not by calling it yourself:
///
/// # Errors
///
/// Propagates errors from the serializer or an element's `Serialize`
/// implementation. With ASONB, the field schema must describe a List rather
/// than the default array Tuple.
///
/// # Examples
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
/// Successful conversion moves values into `[T; N]` without requiring `T: Copy`
/// or `T: Clone`. Borrowed element types may borrow from the input when the
/// underlying deserializer permits it.
///
/// The `#[serde(with = "asonb::array")]` example in [`serialize`] uses both
/// adapter functions, keeping the Serde mapping and derived schema in sync.
///
/// # Errors
///
/// Returns the deserializer's error type if the input is not a suitable
/// sequence, any element cannot be decoded, or the final count differs from
/// `N`. There is no adapter-imposed allocation or element-count quota.
///
/// # Examples
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
