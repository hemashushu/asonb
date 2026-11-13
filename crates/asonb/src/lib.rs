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
//! [`derive@Schema`] alongside Serde's traits for named structs and enumerations:
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
//! | Task | API | Result |
//! | --- | --- | --- |
//! | Encode a Rust value | [`ser_to_bytes`], [`ser_to_writer`] | One complete document |
//! | Decode a Rust value | [`de_from_slice`], [`de_from_reader`] | One compatible Serde value |
//! | Choose wire types and framing | [`ser_value_to_writer`] | A document described by a caller-supplied schema |
//! | Inspect a complete document | [`de_value_from_slice`], [`de_value_from_reader`] | Its schema and a [`Value`] tree |
//! | Produce or consume an incremental List | [`list_stream_writer`], [`list_stream_reader`] | A MultiChunk root List |
//! | Stream a successful List or return a business error | [`result_stream_writer`], [`result_stream_reader`] | A root `Result` enumeration |
//! | Inspect a document with optional List streaming | [`value_document_reader`] | Owned dynamic values and schema access |
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
//! Use [`result_stream_writer`] and [`result_stream_reader`] for the same
//! pipeline wrapped in `Result<Vec<T>, E>`. The successful nested List requires
//! an explicit end marker; business errors are decoded in full before returning.
//! [`value_document_reader`] makes the same List/non-List distinction while
//! returning dynamic values rather than a predetermined Rust type; it also
//! streams the MultiChunk List inside a root `Result::Ok`.
//! Dropping a stream does not finish writing or drain unread input. Consume
//! readers to exhaustion to detect malformed tails and missing nested markers.
//! To read consecutive explicitly terminated streams, pass `&mut reader` and
//! keep using that same reader after the iterator has been consumed or dropped.
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
//! # Schema and Serde Compatibility
//!
//! [`Schema`] describes the serialization representation, not a Rust memory
//! layout. The derive understands serialization-side Serde renames, static
//! skips, named transparent wrappers, and the adapters above. Custom field
//! adapters need a schema override. Unsupported layouts, including conditional
//! skips (`skip_serializing_if`), `flatten`, and tagged or untagged enums, are
//! rejected.
//! See [`derive@Schema`] and [`Schema::schema`] for the customization contract.
//!
//! Decoding requires compatible wire types, not the original Rust type or its
//! [`Schema`] implementation. Numeric widths are not widened or narrowed.
//! Object fields and enumeration variants are matched by the names stored in
//! the document, so their Rust declaration order may differ. Serde controls
//! missing-field defaults and unknown-field policy; ignored fields are still
//! parsed and validated.
//!
//! Standalone unit values, empty Tuples or Objects, and ordinary unit, newtype,
//! or tuple structs have no supported mapping. Unit enumeration variants are
//! supported. A named `#[serde(transparent)]` wrapper can instead use its single
//! serialized field's representation. Deriving [`Schema`] does not implement
//! Serde's traits or make an otherwise unsupported Serde operation valid.
//!
//! # Errors and Resource Policy
//!
//! Schemas and values have no library-imposed depth, size, count, or buffer
//! quota. Recursive Rust schemas and cyclic document schema references are
//! rejected. Wire lengths and platform capacity are checked, but normal Rust
//! allocation and stack limits still apply. Applications must provide their own
//! resource policy for untrusted documents.
//! Single-document readers check EOF and can block on live sources. Typed List
//! and Result streaming stop at their payload boundary without checking EOF.
//! The dynamic reader does so only in streaming mode; other roots, including
//! `Result::Err`, are fully materialized and EOF-checked during construction.
//! This crate does not implement ASON text conversion or graph/pointer identity.
//! Failures use [`AsonbError`]; read errors include a byte offset when one is
//! known. A business error returned as [`ResultStreamReader::Err`] is a decoded
//! value, not a malformed-document or I/O error.
//! Write failures can leave partial output, so discard or otherwise handle that
//! output rather than assuming a failed write was rolled back.

mod binary;
mod de;
mod error;
mod schema;
mod ser;
mod value;

#[cfg(test)]
mod test_utils;

// extern crate self as asonb;

pub mod array;
pub mod date_time;
pub use asonb_derive::Schema;
pub use error::AsonbError;
pub use schema::{DocumentSchema, Encoding, Field, Schema, SchemaBuilder, Type, Variant};
pub use value::{Value, VariantValue};

pub use de::{
    ListStreamReader, ResultStreamReader, ValueDocumentReader, de_from_reader, de_from_slice,
    de_value_from_reader, de_value_from_slice, list_stream_reader, result_stream_reader,
    value_document_reader,
};

pub use ser::{
    ListStreamWriter, ResultStreamWriter, list_stream_writer, result_stream_writer, ser_to_bytes,
    ser_to_writer, ser_value_to_writer,
};
