// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
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
//! the supported wire encodings.
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
//! # Choosing an API
//!
//! Use [`ser_to_bytes`] or [`ser_to_writer`] for ordinary Serde values. Derive
//! [`Schema`] alongside Serde's traits for named structs and enumerations:
//!
//! ```
//! #[derive(serde::Serialize, serde::Deserialize, asonb::Schema)]
//! struct Reading {
//!     sensor_id: u32,
//!     temperature: f64,
//! }
//! let bytes = asonb::ser_to_bytes(&Reading { sensor_id: 7, temperature: 21.5 })?;
//! let decoded: Reading = asonb::de_from_slice(&bytes)?;
//! assert_eq!(decoded.sensor_id, 7);
//! # Ok::<(), asonb::AsonbError>(())
//! ```
//!
//! Each document contains a header, a complete [`DocumentSchema`], and a root
//! payload. The schema describes types rather than instances: empty Lists still
//! know their element type, and enums still describe unselected branches. This
//! is why serialization needs [`Schema`] as well as Serde's `Serialize`, while
//! a reader needs only a compatible `Deserialize` implementation. A reference
//! table reuses type descriptions; it never encodes shared object identity.
//!
//! Use [`list_stream_writer`] and [`list_stream_reader`] for a producer/consumer
//! pipeline whose total element count is not known in advance. These operate on
//! a MultiChunk root List. Writers require an explicit start and end; readers
//! stop at the marker without probing bytes from a following document.
//! [`value_document_reader`] makes the same List/non-List distinction while
//! returning dynamic values rather than a predetermined Rust type.
//!
//! Use [`Value`] with [`DocumentSchema`] when the application must choose Long
//! or MultiChunk framing explicitly or inspect data without a Rust model.
//! [`ser_value_to_writer`] checks that the supplied value matches that schema;
//! [`de_value_from_slice`] can borrow contiguous text and byte payloads.
//!
//! # Representation Choices
//!
//! Rust arrays use positional Tuples by default. The [`mod@array`] adapter selects
//! homogeneous Lists and checks the array length when reading. Ordinary `Vec<u8>`
//! also means a List; use `serde_bytes` for Byte Data. The [`date_time`] adapter
//! selects an eight-byte UTC timestamp rather than chrono's usual Serde string.
//! Adapters and handwritten Serde implementations must agree with their schema;
//! custom field encodings can supply `#[asonb(schema_with = "path")]`.
//!
//! # Errors and Resource Policy
//!
//! Schemas and values have no library-imposed depth, size, count, or buffer
//! quota. Recursive Rust schemas and cyclic document schema references are
//! rejected. Platform capacity, allocation failures, and wire framing are still
//! checked. Applications must provide their own policy for untrusted documents.
//! Single-document readers check EOF and can block on live sources; list
//! streaming stops at its marker.
//! This crate does not implement ASON text conversion or graph/pointer identity.
//! Read failures use [`AsonbError`] and include a byte offset when one is known.
//! Write failures can leave partial output, so discard or otherwise handle that
//! output rather than assuming a failed write was rolled back.

pub mod array;
pub mod date_time;

mod binary;
mod de;
mod error;
mod schema;
mod ser;
mod value;

#[cfg(test)]
mod test_utils;

extern crate self as asonb;

pub use asonb_derive::Schema;
pub use error::AsonbError;
pub use schema::{DocumentSchema, Encoding, Field, Schema, SchemaBuilder, Type, Variant};

pub use de::{
    ListStreamReader, ValueDocumentReader, de_from_reader, de_from_slice, list_stream_reader,
    value_document_reader,
};
pub use ser::{ListStreamWriter, list_stream_writer, ser_to_bytes, ser_to_writer};
pub use value::{Value, de_value_from_reader, de_value_from_slice, ser_value_to_writer};
