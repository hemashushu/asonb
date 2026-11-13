// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Explicit chrono adapter for the 8-byte UTC timestamp DateTime representation.
//!
//! Use `#[serde(with = "asonb::date_time")]` on a
//! `chrono::DateTime<chrono::FixedOffset>` field, or [`DateTime`] for a root
//! value. The [`derive@crate::Schema`] macro recognizes the field adapter and
//! selects [`crate::Type::DateTime`]. Handwritten schemas must select that same
//! scalar class; an `I64` or String schema is not interchangeable with DateTime.
//!
//! This is an explicit opt-in to a binary timestamp, not chrono's ordinary
//! textual Serde representation. The wire retains only signed whole seconds
//! since the Unix epoch. Decoding normalizes the offset to UTC, and encoding
//! rejects rather than rounds subsecond or leap-second values.

use crate::{AsonbError, Schema, SchemaBuilder, Type};
use chrono::{DateTime as ChronoDateTime, FixedOffset};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Internal Serde newtype marker for routing the chrono adapter through the
/// DateTime-specific serializer and deserializer paths.
///
/// Serde passes this name to `serialize_newtype_struct` and
/// `deserialize_newtype_struct`; ASONB recognizes it to validate the
/// `Type::DateTime` schema and interpret the adapter payload as a DateTime,
/// rather than as an ordinary Serde tuple. The marker is only an in-process
/// dispatch aid: it is not a schema entry or bytes in an ASONB document. The
/// document contains the DateTime type definition and its 8-byte payload.
pub(crate) const MARKER: &str = "$asonb::DateTime";

/// A chrono timestamp that uses ASONB's binary DateTime representation.
///
/// The wire stores signed seconds since the Unix epoch, not the original UTC
/// offset. Reading therefore returns the same instant at offset `+00:00`.
/// Fractional seconds, leap seconds, and offsets outside ASON's minute-aligned
/// range of `-23:59` through `+23:59` are rejected rather than rounded or
/// silently changed.
///
/// The public tuple constructor does not validate the timestamp; validation
/// occurs when it is serialized. This wrapper implements [`Schema`],
/// `Serialize`, and `Deserialize`, so it can be used as a root value or an
/// ordinary field without a `#[serde(with = "...")]` attribute. Decoding also
/// requires that the wire timestamp fit chrono's supported date range; use
/// [`crate::Value::DateTime`] when the full `i64` range is needed.
///
/// # Examples
///
/// ```
/// let instant = chrono::DateTime::parse_from_rfc3339("2026-01-01T08:00:00+08:00")?;
/// let bytes = asonb::ser_to_bytes(&asonb::date_time::DateTime(instant))?;
/// let decoded: asonb::date_time::DateTime = asonb::de_from_slice(&bytes)?;
/// assert_eq!(decoded.0.to_rfc3339(), "2026-01-01T00:00:00+00:00");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTime(
    /// The instant to encode, or the UTC-normalized instant just decoded.
    ///
    /// Its original offset is checked on encoding but is not preserved on
    /// the wire. Mutating this field does not run validation.
    pub ChronoDateTime<FixedOffset>,
);

impl Schema for DateTime {
    /// Selects the scalar DateTime schema; no reference entries are needed.
    fn schema(_: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::DateTime)
    }
}
impl Serialize for DateTime {
    /// Passes the wrapped timestamp through the field adapter's validation.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize(&self.0, serializer)
    }
}
impl<'de> Deserialize<'de> for DateTime {
    /// Reads the binary timestamp and wraps the resulting UTC chrono value.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize(deserializer).map(Self)
    }
}

/// Writes a chrono value through ASONB's DateTime-specific Serde path.
///
/// `value` supplies the instant and the offset to validate. `serializer` is the
/// destination provided by Serde. Only whole seconds and valid ASON UTC offsets
/// are accepted. The original offset is not written to the document.
/// Use this through `#[serde(with = "asonb::date_time")]` together with the
/// matching [`deserialize`] function and DateTime schema.
///
/// # Errors
///
/// Returns `S::Error` if `value` contains nonzero subsecond nanoseconds
/// (including a chrono leap second), its offset is not a whole number of
/// minutes within the inclusive `-23:59..=+23:59` range, or the serializer
/// rejects the DateTime representation or fails to write it. No rounding or
/// conversion to a String is attempted.
///
/// # Examples
///
/// ```
/// #[derive(serde::Serialize, asonb::Schema)]
/// struct Event {
///     #[serde(with = "asonb::date_time")]
///     at: chrono::DateTime<chrono::FixedOffset>,
/// }
/// let event = Event { at: chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")? };
/// let bytes = asonb::ser_to_bytes(&event)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn serialize<S: Serializer>(
    value: &ChronoDateTime<FixedOffset>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if value.timestamp_subsec_nanos() != 0 {
        return Err(serde::ser::Error::custom(
            "ASONB DateTime only supports whole seconds.",
        ));
    }
    if !is_valid_ason_utc_offset(value.offset().local_minus_utc()) {
        return Err(serde::ser::Error::custom("Invalid ASON UTC offset."));
    }
    // The marker selects DateTime handling. The one-element tuple carries the
    // i64 through Serde without exposing a custom serializer trait to callers.
    serializer.serialize_newtype_struct(MARKER, &(value.timestamp(),))
}

/// Reads a binary DateTime as a chrono value with a zero UTC offset.
///
/// `deserializer` must support the DateTime newtype marker used by this adapter.
/// The wire timestamp must fit chrono's narrower supported date range; raw
/// [`crate::Value::DateTime`] can represent timestamps that chrono cannot.
/// The returned value always has zero subsecond nanoseconds and offset `+00:00`,
/// regardless of the offset of the originally serialized value.
///
/// # Errors
///
/// Returns `D::Error` if the deserializer cannot provide a binary DateTime,
/// the eight-byte timestamp is malformed or truncated, or its signed-second
/// value is outside chrono's supported range. The corresponding wire schema
/// must be DateTime, not an ordinary `I64`, Tuple, or String.
///
/// # Examples
///
/// ```
/// #[derive(serde::Serialize, serde::Deserialize, asonb::Schema)]
/// struct Event {
///     #[serde(with = "asonb::date_time")]
///     at: chrono::DateTime<chrono::FixedOffset>,
/// }
/// let event = Event { at: chrono::DateTime::parse_from_rfc3339("2026-01-01T08:00:00+08:00")? };
/// let bytes = asonb::ser_to_bytes(&event)?;
/// let decoded: Event = asonb::de_from_slice(&bytes)?;
/// assert_eq!(decoded.at.offset().local_minus_utc(), 0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ChronoDateTime<FixedOffset>, D::Error> {
    /// Converts the adapter's private one-element tuple into a chrono instant.
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = ChronoDateTime<FixedOffset>;
        /// Describes the expected value in Serde's type-mismatch errors.
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an ASONB DateTime")
        }
        /// Checks chrono's range after the wire-level i64 has been decoded.
        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            let (seconds,) = <(i64,)>::deserialize(deserializer)?;
            let instant = ChronoDateTime::from_timestamp(seconds, 0).ok_or_else(|| {
                serde::de::Error::custom("DateTime exceeds chrono's timestamp range.")
            })?;
            let offset = FixedOffset::east_opt(0).unwrap();
            Ok(instant.with_timezone(&offset))
        }
    }
    deserializer.deserialize_newtype_struct(MARKER, Visitor)
}

/// ASON permits minute-aligned offsets through 23:59 in either direction.
const MAX_ASON_UTC_OFFSET_SECONDS: i32 = 23 * 60 * 60 + 59 * 60;

/// Accepts only whole-minute offsets within ASON's inclusive +/-23:59 range.
pub(crate) fn is_valid_ason_utc_offset(offset: i32) -> bool {
    (-MAX_ASON_UTC_OFFSET_SECONDS..=MAX_ASON_UTC_OFFSET_SECONDS).contains(&offset)
        && offset % 60 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// Checks that explicit adapters select scalars rather than Serde strings/lists.
    fn test_schema() {
        assert_eq!(
            crate::DocumentSchema::of::<DateTime>().unwrap().root,
            Type::DateTime
        );
        assert_eq!(
            crate::DocumentSchema::of::<serde_bytes::ByteBuf>()
                .unwrap()
                .root,
            Type::Bytes(crate::Encoding::Regular)
        );
    }

    #[test]
    /// Checks UTC normalization, negative timestamps, and every truncated prefix.
    fn test_binary_round_trips() {
        for seconds in [-1, 0, 1] {
            for offset in [-86_340, 0, 28_800, 86_340] {
                let value = DateTime(
                    ChronoDateTime::from_timestamp(seconds, 0)
                        .unwrap()
                        .with_timezone(&FixedOffset::east_opt(offset).unwrap()),
                );
                let bytes = crate::ser_to_bytes(&value).unwrap();
                assert_eq!(&bytes[13..21], &value.0.timestamp().to_le_bytes());
                let expected = DateTime(
                    ChronoDateTime::from_timestamp(value.0.timestamp(), 0)
                        .unwrap()
                        .with_timezone(&FixedOffset::east_opt(0).unwrap()),
                );
                assert_eq!(crate::de_from_slice::<DateTime>(&bytes).unwrap(), expected);
                for end in 0..bytes.len() {
                    assert!(crate::de_from_slice::<DateTime>(&bytes[..end]).is_err());
                }
            }
        }
    }

    #[test]
    /// Keeps wire-range support separate from chrono and ASON offset restrictions.
    fn test_invalid_and_unrepresentable_date_times() {
        let bytes = crate::test_utils::document(Type::DateTime, &i64::MAX.to_le_bytes());
        assert!(crate::de_from_slice::<DateTime>(&bytes).is_err());
        assert_eq!(
            crate::de_value_from_slice(&bytes).unwrap().1,
            crate::Value::DateTime(i64::MAX)
        );
        let leap = DateTime(
            ChronoDateTime::from_timestamp(59, 1_000_000_000)
                .unwrap()
                .with_timezone(&FixedOffset::east_opt(0).unwrap()),
        );
        assert!(crate::ser_to_bytes(&leap).is_err());

        let fractional = DateTime(
            ChronoDateTime::from_timestamp(0, 1)
                .unwrap()
                .with_timezone(&FixedOffset::east_opt(0).unwrap()),
        );
        assert!(crate::ser_to_bytes(&fractional).is_err());

        let second_precision_offset = DateTime(
            ChronoDateTime::from_timestamp(0, 0)
                .unwrap()
                .with_timezone(&FixedOffset::east_opt(1).unwrap()),
        );
        assert!(crate::ser_to_bytes(&second_precision_offset).is_err());

        for offset in [-86_399, 86_399] {
            let out_of_range_offset = DateTime(
                ChronoDateTime::from_timestamp(0, 0)
                    .unwrap()
                    .with_timezone(&FixedOffset::east_opt(offset).unwrap()),
            );
            assert!(crate::ser_to_bytes(&out_of_range_offset).is_err());
        }
    }
}
