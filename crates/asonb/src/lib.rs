// Copyright (c) 2025 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Schema-described binary serialization for ASON values.
//!
//! Serialization requires both Serde's `Serialize` and [`Schema`]. Reading
//! uses the schema in the document and requires only Serde's `Deserialize`.
//! [`de_from_slice`] can safely borrow contiguous string and byte payloads;
//! [`de_from_reader`] returns owned values. Low-level [`Value`] APIs expose
//! every wire encoding, including uninterpreted fixed-capacity slots.
//!
//! ```
//! let bytes = asonb::ser_to_bytes(&vec![11i32, 13])?;
//! let values: Vec<i32> = asonb::de_from_slice(&bytes)?;
//! assert_eq!(values, [11, 13]);
//! # Ok::<(), asonb::AsonbError>(())
//! ```
//!
//! ```
//! let bytes = asonb::ser_to_bytes(&"Hello")?;
//! let text: &str = asonb::de_from_slice(&bytes)?;
//! assert_eq!(text, "Hello");
//! # Ok::<(), asonb::AsonbError>(())
//! ```
//!
//! Schemas and values have no library-imposed depth, size, count, or buffer
//! quota. Recursive Rust schemas and cyclic document schema references are
//! rejected. Platform capacity, allocation failures, and wire framing are still
//! checked. Applications must provide their own policy for untrusted documents.
//! Single-document readers check EOF and can block on live sources; list
//! streaming stops at its marker.
//! This crate does not implement ASON text conversion or graph/pointer identity.

mod binary;
pub mod date_time;
mod de;
mod error;
mod schema;
mod ser;
#[cfg(test)]
mod test_utils;
mod value;

extern crate self as asonb;

pub use asonb_derive::Schema;

pub use de::{ListStreamReader, de_from_reader, de_from_slice, list_stream_reader};
pub use error::AsonbError;
pub use schema::{DocumentSchema, Encoding, Field, Schema, SchemaBuilder, Type, Variant};
pub use ser::{ListStreamWriter, list_stream_writer, ser_to_bytes, ser_to_writer};
pub use value::{Value, de_value_from_reader, de_value_from_slice, ser_value_to_writer};
