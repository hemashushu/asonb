// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Explicit chrono adapter for the 16-byte binary DateTime representation.
//! Use `#[serde(with = "asonb::date_time")]` on a chrono field, or `DateTime`
//! for a root value. Ordinary chrono Serde strings remain ordinary Strings.

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
/// document contains the DateTime type definition and its 16-byte payload.
pub(crate) const MARKER: &str = "$asonb::DateTime";

/// A root-friendly chrono value with a binary rather than text Serde mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTime(pub ChronoDateTime<FixedOffset>);

impl Schema for DateTime {
    fn schema(_: &mut SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::DateTime)
    }
}
impl Serialize for DateTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize(&self.0, serializer)
    }
}
impl<'de> Deserialize<'de> for DateTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserialize(deserializer).map(Self)
    }
}

pub fn serialize<S: Serializer>(
    value: &ChronoDateTime<FixedOffset>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if value.timestamp_subsec_nanos() >= 1_000_000_000 {
        return Err(serde::ser::Error::custom(
            "ASONB does not encode leap seconds.",
        ));
    }
    serializer.serialize_newtype_struct(
        MARKER,
        &(
            value.timestamp(),
            value.timestamp_subsec_nanos(),
            value.offset().local_minus_utc(),
        ),
    )
}

pub fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ChronoDateTime<FixedOffset>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = ChronoDateTime<FixedOffset>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an ASONB DateTime")
        }
        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            let (seconds, nanos, offset) = <(i64, u32, i32)>::deserialize(deserializer)?;
            if nanos >= 1_000_000_000 {
                return Err(serde::de::Error::custom("Invalid nanoseconds."));
            }
            let instant = ChronoDateTime::from_timestamp(seconds, nanos).ok_or_else(|| {
                serde::de::Error::custom("DateTime exceeds chrono's timestamp range.")
            })?;
            if !crate::value::is_valid_ason_utc_offset(offset) {
                return Err(serde::de::Error::custom("Invalid ASON UTC offset."));
            }
            let offset = FixedOffset::east_opt(offset)
                .ok_or_else(|| serde::de::Error::custom("UTC offset exceeds chrono's range."))?;
            Ok(instant.with_timezone(&offset))
        }
    }
    deserializer.deserialize_newtype_struct(MARKER, Visitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
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
    fn test_binary_round_trips() {
        for seconds in [-1, 0, 1] {
            for nanos in [0, 500_000_000, 999_999_999] {
                for offset in [-86_340, 0, 28_800, 86_340] {
                    let value = DateTime(
                        ChronoDateTime::from_timestamp(seconds, nanos)
                            .unwrap()
                            .with_timezone(&FixedOffset::east_opt(offset).unwrap()),
                    );
                    let bytes = crate::ser_to_bytes(&value).unwrap();
                    assert_eq!(&bytes[13..21], &seconds.to_le_bytes());
                    assert_eq!(&bytes[21..25], &nanos.to_le_bytes());
                    assert_eq!(&bytes[25..29], &offset.to_le_bytes());
                    assert_eq!(crate::de_from_slice::<DateTime>(&bytes).unwrap(), value);
                    for end in 0..bytes.len() {
                        assert!(crate::de_from_slice::<DateTime>(&bytes[..end]).is_err());
                    }
                }
            }
        }
    }

    #[test]
    fn test_invalid_and_unrepresentable_date_times() {
        for (seconds, nanos, offset) in [
            (0, 1_000_000_000, 0i32),
            (0, u32::MAX, 0),
            (i64::MAX, 0, 0),
            (0, 0, 1),
            (0, 0, -1),
            (0, 0, 86_341),
            (0, 0, -86_341),
            (0, 0, 86_400),
            (0, 0, -86_400),
        ] {
            let mut payload = seconds.to_le_bytes().to_vec();
            payload.extend_from_slice(&nanos.to_le_bytes());
            payload.extend_from_slice(&offset.to_le_bytes());
            let bytes = crate::test_utils::document(Type::DateTime, &payload);
            assert!(crate::de_from_slice::<DateTime>(&bytes).is_err());
            if nanos < 1_000_000_000 && (-86_340..=86_340).contains(&offset) && offset % 60 == 0 {
                assert!(crate::de_value_from_slice(&bytes).is_ok());
            } else {
                assert!(crate::de_value_from_slice(&bytes).is_err());
            }
        }
        let leap = DateTime(
            ChronoDateTime::from_timestamp(59, 1_000_000_000)
                .unwrap()
                .with_timezone(&FixedOffset::east_opt(0).unwrap()),
        );
        assert!(crate::ser_to_bytes(&leap).is_err());

        let second_precision_offset = DateTime(
            ChronoDateTime::from_timestamp(0, 0)
                .unwrap()
                .with_timezone(&FixedOffset::east_opt(1).unwrap()),
        );
        assert!(crate::ser_to_bytes(&second_precision_offset).is_err());
    }
}
