// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Explicit chrono adapter for the 8-byte UTC timestamp DateTime representation.
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
/// document contains the DateTime type definition and its 8-byte payload.
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
    if value.timestamp_subsec_nanos() != 0 {
        return Err(serde::ser::Error::custom(
            "ASONB DateTime only supports whole seconds.",
        ));
    }
    if !crate::value::is_valid_ason_utc_offset(value.offset().local_minus_utc()) {
        return Err(serde::ser::Error::custom("Invalid ASON UTC offset."));
    }
    serializer.serialize_newtype_struct(MARKER, &(value.timestamp(),))
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
    }
}
