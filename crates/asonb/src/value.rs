// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Explicit values for applications that select their own wire schema.

use crate::{
    AsonbError, DocumentSchema, Encoding, Type, Variant,
    binary::{HEADER, Input, ReaderInput, SliceInput, write_all, write_u32},
};
use std::{
    borrow::Cow,
    io::{Read, Write},
};

/// ASON permits minute-aligned offsets through 23:59 in either direction.
const MAX_ASON_UTC_OFFSET_SECONDS: i32 = 23 * 60 * 60 + 59 * 60;

/// A value interpreted according to a separate document schema.
///
/// Container values do not repeat field names, element types, or encoding modes:
/// those belong to [`DocumentSchema`]. Strings and bytes may borrow from a slice
/// document; containers own their child storage. Enum indices refer to schema
/// order, not Rust discriminant values.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'de> {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Bool(bool),
    Char(char),
    DateTime(i64),
    String(Cow<'de, str>),
    Bytes(Cow<'de, [u8]>),
    List(Vec<Value<'de>>),
    Map(Vec<(Value<'de>, Value<'de>)>),
    Tuple(Vec<Value<'de>>),
    Object(Vec<Value<'de>>),
    Enum(u32, Box<Value<'de>>),
}

/// Writes a complete document using a caller-selected schema and dynamic value.
///
/// `schema` supplies types, field order, and framing. `value` must match it
/// exactly; numeric widths are not coerced. `writer` receives the header, schema,
/// and payload, but is not flushed. Invalid schemas are rejected before writing.
/// Value mismatches and I/O failures can leave partial output: arbitrary writers
/// cannot be rolled back. There are no library-imposed size or depth quotas.
///
/// ```
/// use asonb::{DocumentSchema, Encoding, Type, Value};
/// let schema = DocumentSchema::new(Type::String(Encoding::Long));
/// let mut bytes = Vec::new();
/// asonb::ser_value_to_writer(&schema, &Value::String("hello".into()), &mut bytes)?;
/// assert_eq!(asonb::de_value_from_slice(&bytes)?.1, Value::String("hello".into()));
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn ser_value_to_writer(
    schema: &DocumentSchema,
    value: &Value<'_>,
    writer: &mut impl Write,
) -> Result<(), AsonbError> {
    schema.validate()?;
    write_all(writer, &HEADER)?;
    schema.write(writer)?;
    write_value(writer, schema, &schema.root, value)
}

/// Reads one finite document into its schema and an entirely owned value tree.
///
/// `reader` starts at the document header and is consumed by this call. The
/// returned value has no borrow from the source. Invalid schemas, malformed
/// payloads, I/O errors, and trailing bytes are rejected. The EOF check may block
/// on a live source; use [`crate::list_stream_reader`] for pipelines instead.
///
/// ```
/// let bytes = asonb::ser_to_bytes(&42i32)?;
/// let (schema, value) = asonb::de_value_from_reader(bytes.as_slice())?;
/// assert_eq!(schema.root, asonb::Type::I32);
/// assert_eq!(value, asonb::Value::I32(42));
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn de_value_from_reader(
    reader: impl Read,
) -> Result<(DocumentSchema, Value<'static>), AsonbError> {
    read_document(&mut ReaderInput::new(reader))
}

/// Reads one complete document, borrowing contiguous string and byte payloads.
///
/// `bytes` must contain exactly one document, including its header. Returned
/// borrows remain valid only while this slice lives. Chunked text and byte data
/// are joined into owned storage; schemas and container vectors are also owned.
/// Malformed input and trailing bytes return an error. Numeric values are copied
/// from little-endian bytes, never viewed through unaligned native references.
///
/// ```
/// use std::borrow::Cow;
/// let bytes = asonb::ser_to_bytes(&"hello")?;
/// let (_, value) = asonb::de_value_from_slice(&bytes)?;
/// assert!(matches!(value, asonb::Value::String(Cow::Borrowed("hello"))));
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn de_value_from_slice(bytes: &[u8]) -> Result<(DocumentSchema, Value<'_>), AsonbError> {
    read_document(&mut SliceInput::new(bytes))
}

/// Shares header/schema/value/EOF checks across borrowed and owned input sources.
fn read_document<'de>(
    input: &mut impl Input<'de>,
) -> Result<(DocumentSchema, Value<'de>), AsonbError> {
    input.header()?;
    let schema = DocumentSchema::read(input)?;
    let value = read_value(input, &schema, &schema.root, true)?;
    input.finish()?;
    Ok((schema, value))
}

/// Writes a count in the selected framing; MultiChunk means one chunk count.
pub(crate) fn write_length(
    writer: &mut impl Write,
    encoding: &Encoding,
    count: u64,
) -> Result<(), AsonbError> {
    match encoding {
        Encoding::Regular | Encoding::MultiChunk => write_u32(writer, count),
        Encoding::Long => write_all(writer, &count.to_le_bytes()),
    }
}

/// Reads a single regular/long length; chunk termination needs Input::chunk.
pub(crate) fn read_length<'de>(
    input: &mut impl Input<'de>,
    encoding: &Encoding,
) -> Result<u64, AsonbError> {
    match encoding {
        Encoding::Regular => Ok(input.u32()? as u64),
        Encoding::Long => input.u64(),
        Encoding::MultiChunk => Err(AsonbError::Message("Expected a chunk boundary.".into())),
    }
}

/// Reads text or byte data, joining chunks only when framing requires it.
/// `string` validates UTF-8, and `root` permits EOF between root-level chunks.
pub(crate) fn read_blob<'de>(
    input: &mut impl Input<'de>,
    encoding: &Encoding,
    string: bool,
    root: bool,
) -> Result<Cow<'de, [u8]>, AsonbError> {
    if *encoding != Encoding::MultiChunk {
        let length = read_length(input, encoding)?;
        let bytes = input.payload(length)?;
        if string {
            validate_utf8(&bytes, input.offset() - length)?;
        }
        return Ok(bytes);
    }
    let mut output = Vec::new();
    loop {
        let length = input.chunk(root)? as u64;
        if length == 0 {
            break;
        }
        let bytes = input.payload(length)?;
        // Every text chunk must be valid UTF-8 on its own. A multibyte character
        // cannot be split across chunks even if the joined bytes would be valid.
        if string {
            validate_utf8(&bytes, input.offset() - length)?;
        }
        output
            .try_reserve(bytes.len())
            .map_err(|error| AsonbError::Message(error.to_string()))?;
        output.extend_from_slice(&bytes);
    }
    Ok(Cow::Owned(output))
}

/// Reports invalid UTF-8 at the starting document offset of this payload.
fn validate_utf8(bytes: &[u8], offset: u64) -> Result<(), AsonbError> {
    std::str::from_utf8(bytes)
        .map(|_| ())
        .map_err(|error| AsonbError::MessageWithOffset(error.to_string(), offset))
}

/// Accepts only whole-minute offsets within ASON's inclusive +/-23:59 range.
pub(crate) fn is_valid_ason_utc_offset(offset: i32) -> bool {
    (-MAX_ASON_UTC_OFFSET_SECONDS..=MAX_ASON_UTC_OFFSET_SECONDS).contains(&offset)
        && offset % 60 == 0
}

/// Turns a variant shape into the type used by the shared payload machinery.
/// The empty unit Tuple is internal: it is not a standalone document schema.
pub(crate) fn payload_type(variant: &Variant) -> Type {
    match variant {
        Variant::Unit(_) => Type::Tuple(vec![]),
        Variant::Newtype(_, ty) => ty.clone(),
        Variant::Tuple(_, types) => Type::Tuple(types.clone()),
        Variant::Struct(_, fields) => Type::Object(String::new(), fields.clone()),
    }
}

/// Checks a dynamic value against its resolved type while emitting its payload.
/// Container headers may already be written when a later child fails validation.
pub(crate) fn write_value(
    writer: &mut impl Write,
    schema: &DocumentSchema,
    ty: &Type,
    value: &Value<'_>,
) -> Result<(), AsonbError> {
    let ty = schema.resolve(ty.clone())?;
    /// Shares exact-width integer writing without introducing numeric coercions.
    macro_rules! number {
        ($variant:ident,$value:expr) => {
            if let Value::$variant(value) = $value {
                return write_all(writer, &value.to_le_bytes());
            }
        };
    }
    match &ty {
        Type::I8 => number!(I8, value),
        Type::I16 => number!(I16, value),
        Type::I32 => number!(I32, value),
        Type::I64 => number!(I64, value),
        Type::U8 => number!(U8, value),
        Type::U16 => number!(U16, value),
        Type::U32 => number!(U32, value),
        Type::U64 => number!(U64, value),
        Type::F32 => {
            if let Value::F32(value) = value {
                // ASONB writers use a stable NaN representation, but retain
                // ordinary values, infinities, and signed zero exactly.
                let bits = if value.is_nan() {
                    0x7fc00000
                } else {
                    value.to_bits()
                };
                return write_all(writer, &bits.to_le_bytes());
            }
        }
        Type::F64 => {
            if let Value::F64(value) = value {
                let bits = if value.is_nan() {
                    0x7ff8000000000000
                } else {
                    value.to_bits()
                };
                return write_all(writer, &bits.to_le_bytes());
            }
        }
        Type::Bool => {
            if let Value::Bool(value) = value {
                return write_all(writer, &[u8::from(*value)]);
            }
        }
        Type::Char => {
            if let Value::Char(value) = value {
                return write_all(writer, &(*value as u32).to_le_bytes());
            }
        }
        Type::DateTime => {
            if let Value::DateTime(seconds) = value {
                return write_all(writer, &seconds.to_le_bytes());
            }
        }
        Type::String(encoding) | Type::Bytes(encoding) => {
            let bytes: &[u8] = match (&ty, value) {
                (Type::String(_), Value::String(text)) => text.as_bytes(),
                (Type::Bytes(_), Value::Bytes(bytes)) => bytes,
                _ => {
                    return Err(AsonbError::Message(
                        "String or byte-data value does not match schema.".into(),
                    ));
                }
            };
            if *encoding == Encoding::MultiChunk && bytes.is_empty() {
                return write_u32(writer, 0);
            }
            // A materialized blob is emitted as one chunk plus a terminator.
            // Empty blobs need only the terminator, handled above.
            write_length(writer, encoding, bytes.len() as u64)?;
            write_all(writer, bytes)?;
            if *encoding == Encoding::MultiChunk {
                write_u32(writer, 0)?;
            }
            return Ok(());
        }
        Type::List(encoding, item) => {
            if let Value::List(values) = value {
                if *encoding == Encoding::MultiChunk && values.is_empty() {
                    return write_u32(writer, 0);
                }
                write_length(writer, encoding, values.len() as u64)?;
                for value in values {
                    write_value(writer, schema, item, value)?;
                }
                if *encoding == Encoding::MultiChunk {
                    write_u32(writer, 0)?;
                }
                return Ok(());
            }
        }
        Type::Map(long, key, item) => {
            if let Value::Map(values) = value {
                write_length(
                    writer,
                    &if *long {
                        Encoding::Long
                    } else {
                        Encoding::Regular
                    },
                    values.len() as u64,
                )?;
                for (key_value, value) in values {
                    write_value(writer, schema, key, key_value)?;
                    write_value(writer, schema, item, value)?;
                }
                return Ok(());
            }
        }
        Type::Tuple(types) => {
            if let Value::Tuple(values) = value {
                if types.len() != values.len() {
                    return Err(AsonbError::Message("Tuple arity mismatch.".into()));
                }
                for (ty, value) in types.iter().zip(values) {
                    write_value(writer, schema, ty, value)?;
                }
                return Ok(());
            }
        }
        Type::Object(_, fields) => {
            if let Value::Object(values) = value {
                if fields.len() != values.len() {
                    return Err(AsonbError::Message("Object field count mismatch.".into()));
                }
                for (field, value) in fields.iter().zip(values) {
                    write_value(writer, schema, &field.ty, value)?;
                }
                return Ok(());
            }
        }
        Type::Enum(_, variants) => {
            if let Value::Enum(index, payload) = value {
                let variant = variants
                    .get(*index as usize)
                    .ok_or_else(|| AsonbError::Message("Variant index out of range.".into()))?;
                write_u32(writer, *index as u64)?;
                return write_value(writer, schema, &payload_type(variant), payload);
            }
        }
        Type::Reference(_) => unreachable!(),
    }
    Err(AsonbError::Message(format!("Value does not match {ty:?}.")))
}

/// Materializes a schema-directed value tree, borrowing leaf bytes when possible.
/// Only the actual root may use EOF instead of a MultiChunk terminator; children
/// always receive `false` so a truncated child cannot consume its parent's end.
pub(crate) fn read_value<'de>(
    input: &mut impl Input<'de>,
    schema: &DocumentSchema,
    ty: &Type,
    root: bool,
) -> Result<Value<'de>, AsonbError> {
    let ty = schema.resolve(ty.clone())?;
    Ok(match ty {
        Type::I8 => Value::I8(i8::from_le_bytes(input.exact()?)),
        Type::I16 => Value::I16(i16::from_le_bytes(input.exact()?)),
        Type::I32 => Value::I32(i32::from_le_bytes(input.exact()?)),
        Type::I64 => Value::I64(i64::from_le_bytes(input.exact()?)),
        Type::U8 => Value::U8(u8::from_le_bytes(input.exact()?)),
        Type::U16 => Value::U16(u16::from_le_bytes(input.exact()?)),
        Type::U32 => Value::U32(input.u32()?),
        Type::U64 => Value::U64(input.u64()?),
        Type::F32 => Value::F32(f32::from_le_bytes(input.exact()?)),
        Type::F64 => Value::F64(f64::from_le_bytes(input.exact()?)),
        Type::Bool => Value::Bool(input.exact::<1>()?[0] != 0),
        Type::Char => {
            let offset = input.offset();
            Value::Char(char::from_u32(input.u32()?).ok_or_else(|| {
                AsonbError::MessageWithOffset("Invalid Unicode scalar.".into(), offset)
            })?)
        }
        Type::DateTime => {
            let seconds = i64::from_le_bytes(input.exact()?);
            Value::DateTime(seconds)
        }
        Type::String(encoding) => {
            let bytes = read_blob(input, &encoding, true, root)?;
            Value::String(match bytes {
                Cow::Borrowed(bytes) => Cow::Borrowed(
                    std::str::from_utf8(bytes)
                        .map_err(|error| AsonbError::Message(error.to_string()))?,
                ),
                Cow::Owned(bytes) => Cow::Owned(
                    String::from_utf8(bytes)
                        .map_err(|error| AsonbError::Message(error.to_string()))?,
                ),
            })
        }
        Type::Bytes(encoding) => {
            let bytes = read_blob(input, &encoding, false, root)?;
            Value::Bytes(bytes)
        }
        Type::List(encoding, item) => {
            let mut values = Vec::new();
            loop {
                let count = if encoding == Encoding::MultiChunk {
                    input.chunk(root)? as u64
                } else {
                    read_length(input, &encoding)?
                };
                if encoding == Encoding::MultiChunk && count == 0 {
                    break;
                }
                // Check the combined count across chunks, not just each chunk.
                // This is a platform-capacity check rather than a policy quota.
                let total = (values.len() as u64)
                    .checked_add(count)
                    .ok_or_else(|| AsonbError::Message("Container length overflow.".into()))?;
                value_count::<Value<'de>>(total)?;
                for _ in 0..count {
                    let value = read_value(input, schema, &item, false)?;
                    push_value(&mut values, value)?;
                }
                if encoding != Encoding::MultiChunk {
                    break;
                }
            }
            Value::List(values)
        }
        Type::Map(long, key, item) => {
            let count = read_length(
                input,
                &if long {
                    Encoding::Long
                } else {
                    Encoding::Regular
                },
            )?;
            value_count::<(Value<'de>, Value<'de>)>(count)?;
            let mut values = Vec::new();
            for _ in 0..count {
                let pair = (
                    read_value(input, schema, &key, false)?,
                    read_value(input, schema, &item, false)?,
                );
                push_value(&mut values, pair)?;
            }
            Value::Map(values)
        }
        Type::Tuple(types) => {
            let mut values = Vec::new();
            for ty in types {
                let value = read_value(input, schema, &ty, false)?;
                push_value(&mut values, value)?;
            }
            Value::Tuple(values)
        }
        Type::Object(_, fields) => {
            let mut values = Vec::new();
            for field in fields {
                let value = read_value(input, schema, &field.ty, false)?;
                push_value(&mut values, value)?;
            }
            Value::Object(values)
        }
        Type::Enum(_, variants) => {
            let index = input.u32()?;
            let variant = variants.get(index as usize).ok_or_else(|| {
                AsonbError::MessageWithOffset(
                    "Variant index out of range.".into(),
                    input.offset() - 4,
                )
            })?;
            Value::Enum(
                index,
                Box::new(read_value(input, schema, &payload_type(variant), false)?),
            )
        }
        Type::Reference(_) => unreachable!(),
    })
}

/// Validate and discard an unwanted field without allocating a value tree.
/// Leaf payloads still pass through the same scalar/UTF-8 checks as normal
/// reads, so ignoring a field never accepts malformed input silently.
pub(crate) fn skip_value<'de>(
    input: &mut impl Input<'de>,
    schema: &DocumentSchema,
    ty: &Type,
) -> Result<(), AsonbError> {
    match schema.resolve(ty.clone())? {
        Type::List(encoding, item) => loop {
            let count = if encoding == Encoding::MultiChunk {
                input.chunk(false)? as u64
            } else {
                read_length(input, &encoding)?
            };
            if encoding == Encoding::MultiChunk && count == 0 {
                break;
            }
            for _ in 0..count {
                skip_value(input, schema, &item)?;
            }
            if encoding != Encoding::MultiChunk {
                break;
            }
        },
        Type::Map(long, key, value) => {
            let count = read_length(
                input,
                &if long {
                    Encoding::Long
                } else {
                    Encoding::Regular
                },
            )?;
            for _ in 0..count {
                skip_value(input, schema, &key)?;
                skip_value(input, schema, &value)?;
            }
        }
        Type::Tuple(types) => {
            for ty in types {
                skip_value(input, schema, &ty)?;
            }
        }
        Type::Object(_, fields) => {
            for field in fields {
                skip_value(input, schema, &field.ty)?;
            }
        }
        Type::Enum(_, variants) => {
            let index = input.u32()?;
            let variant = variants.get(index as usize).ok_or_else(|| {
                AsonbError::MessageWithOffset(
                    "Variant index out of range.".into(),
                    input.offset() - 4,
                )
            })?;
            skip_value(input, schema, &payload_type(variant))?;
        }
        ty => {
            read_value(input, schema, &ty, false)?;
        }
    }
    Ok(())
}

/// Check only the platform's representable Vec size, without imposing a quota.
fn value_count<T>(count: u64) -> Result<usize, AsonbError> {
    let count = usize::try_from(count)
        .map_err(|_| AsonbError::Message("Container length does not fit usize.".into()))?;
    let bytes = count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or_else(|| AsonbError::Message("Container allocation size overflow.".into()))?;
    if bytes > isize::MAX as usize {
        return Err(AsonbError::Message(
            "Container allocation exceeds platform capacity.".into(),
        ));
    }
    Ok(count)
}

/// Grows a materialized container fallibly before moving the next value into it.
fn push_value<T>(values: &mut Vec<T>, value: T) -> Result<(), AsonbError> {
    values
        .try_reserve(1)
        .map_err(|error| AsonbError::Message(error.to_string()))?;
    values.push(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Allows large valid counts while rejecting impossible platform allocations.
    fn test_platform_container_capacity() {
        assert_eq!(value_count::<Value<'_>>(0).unwrap(), 0);
        assert!(value_count::<Value<'_>>(u64::MAX).is_err());
        assert!(value_count::<Value<'_>>(1_000_001).is_ok());
    }

    /// Checks both ownership modes and truncations, except legal root chunk EOFs.
    fn round_trip(ty: Type, value: Value<'_>) {
        let schema = DocumentSchema::new(ty);
        let mut bytes = Vec::new();
        ser_value_to_writer(&schema, &value, &mut bytes).unwrap();
        assert_eq!(de_value_from_slice(&bytes).unwrap().1, value);
        assert_eq!(de_value_from_reader(bytes.as_slice()).unwrap().1, value);
        for end in 0..bytes.len() {
            if matches!(
                schema.root,
                Type::List(Encoding::MultiChunk, _)
                    | Type::String(Encoding::MultiChunk)
                    | Type::Bytes(Encoding::MultiChunk)
            ) {
                continue;
            }
            assert!(de_value_from_slice(&bytes[..end]).is_err());
        }
    }

    #[test]
    /// Covers all framing modes and all four enumeration payload shapes.
    fn test_every_payload_form() {
        for encoding in [Encoding::Regular, Encoding::Long, Encoding::MultiChunk] {
            round_trip(
                Type::String(encoding.clone()),
                Value::String(Cow::Borrowed("a\0b")),
            );
            round_trip(
                Type::Bytes(encoding.clone()),
                Value::Bytes(Cow::Borrowed(&[0xff, 0, 3])),
            );
            round_trip(
                Type::List(encoding, Box::new(Type::I32)),
                Value::List(vec![
                    Value::I32(i32::MIN),
                    Value::I32(0),
                    Value::I32(i32::MAX),
                ]),
            );
        }
        for long in [false, true] {
            round_trip(
                Type::Map(long, Box::new(Type::U8), Box::new(Type::Bool)),
                Value::Map(vec![(Value::U8(1), Value::Bool(true))]),
            );
        }
        round_trip(Type::DateTime, Value::DateTime(i64::MIN));
        let variants = vec![
            Variant::Unit("A".into()),
            Variant::Newtype("B".into(), Type::I8),
            Variant::Tuple("C".into(), vec![Type::Bool, Type::I32]),
            Variant::Struct("D".into(), vec![crate::Field::new("x", Type::Char)]),
        ];
        for (index, payload) in [
            Value::Tuple(vec![]),
            Value::I8(-1),
            Value::Tuple(vec![Value::Bool(true), Value::I32(42)]),
            Value::Object(vec![Value::Char('\u{10ffff}')]),
        ]
        .into_iter()
        .enumerate()
        {
            round_trip(
                Type::Enum("E".into(), variants.clone()),
                Value::Enum(index as u32, Box::new(payload)),
            );
        }
    }

    #[test]
    /// Allows root EOF only at chunk boundaries and rejects invalid text chunks.
    fn test_chunk_boundaries_and_utf8() {
        let schema = DocumentSchema::new(Type::String(Encoding::MultiChunk));
        let mut bytes = Vec::new();
        write_all(&mut bytes, &HEADER).unwrap();
        schema.write(&mut bytes).unwrap();
        let start = bytes.len();
        bytes.extend_from_slice(&[1, 0, 0, 0, b'a']);
        assert_eq!(
            de_value_from_slice(&bytes).unwrap().1,
            Value::String(Cow::Borrowed("a"))
        );
        for end in start + 1..start + 5 {
            assert!(de_value_from_slice(&bytes[..end]).is_err());
        }
        bytes[start + 4] = 0xff;
        assert!(de_value_from_slice(&bytes).is_err());
    }

    #[test]
    /// Rejects non-scalar Unicode values and mismatched dynamic numeric widths.
    fn test_invalid_values() {
        let schema = DocumentSchema::new(Type::Char);
        let mut bytes = Vec::new();
        write_all(&mut bytes, &HEADER).unwrap();
        schema.write(&mut bytes).unwrap();
        for value in [0xd800u32, 0xdfff, 0x110000, u32::MAX] {
            let mut input = bytes.clone();
            input.extend_from_slice(&value.to_le_bytes());
            assert!(de_value_from_slice(&input).is_err());
        }
        assert!(
            ser_value_to_writer(
                &DocumentSchema::new(Type::I32),
                &Value::I64(1),
                &mut Vec::new()
            )
            .is_err()
        );
    }

    #[test]
    /// Distinguishes optional root terminators from mandatory nested terminators.
    fn test_nested_chunks_and_empty_values() {
        for (ty, value) in [
            (Type::String(Encoding::MultiChunk), Value::String("".into())),
            (
                Type::Bytes(Encoding::MultiChunk),
                Value::Bytes(Cow::Borrowed(&[])),
            ),
            (
                Type::List(Encoding::MultiChunk, Box::new(Type::I32)),
                Value::List(vec![]),
            ),
        ] {
            let mut bytes = Vec::new();
            ser_value_to_writer(&DocumentSchema::new(ty), &value, &mut bytes).unwrap();
            assert_eq!(de_value_from_slice(&bytes).unwrap().1, value);
            bytes.truncate(bytes.len() - 4);
            assert_eq!(de_value_from_slice(&bytes).unwrap().1, value);
            bytes.push(0);
            assert!(de_value_from_slice(&bytes).is_err());
        }
        let schema = DocumentSchema::new(Type::List(
            Encoding::Regular,
            Box::new(Type::String(Encoding::MultiChunk)),
        ));
        let mut bytes = Vec::new();
        ser_value_to_writer(
            &schema,
            &Value::List(vec![Value::String("a".into())]),
            &mut bytes,
        )
        .unwrap();
        assert!(de_value_from_slice(&bytes).is_ok());
        bytes.truncate(bytes.len() - 4);
        assert!(de_value_from_slice(&bytes).is_err());
    }
}
