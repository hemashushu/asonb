// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Schema-directed Serde decoding, borrowing from slices where possible.
//! Container accessors enforce complete consumption so a custom visitor cannot
//! silently leave the input cursor in the middle of a value.

use crate::{
    AsonbError, DocumentSchema, Encoding, Field, Type, Value, Variant,
    binary::{Input, ReaderInput, SliceInput},
    binary::{read_blob, read_length},
    schema::payload_type,
    value::VariantValue,
};
use serde::{
    Deserialize,
    de::{
        self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess, VariantAccess,
        Visitor,
    },
};
use std::{borrow::Cow, io::Read, iter::FusedIterator, marker::PhantomData};

/// Reads exactly one finite document into an owned Rust value of type `T`.
///
/// `reader` must start at the header and reach EOF after the root payload. The
/// target needs `DeserializeOwned`, not [`crate::Schema`]: the writer's schema
/// travels with the document. Strings and bytes are owned because a general
/// reader cannot lend stable storage. The root value is fully decoded before
/// returning; this is not an incremental iterator.
///
/// The reader is taken by value; pass `&mut reader` if the caller must retain
/// it. Checking EOF can block on a live source and reads a byte past the payload
/// if trailing data exists. Use [`list_stream_reader`] or
/// [`result_stream_reader`] for consecutive streamed documents instead.
/// No library-imposed resource quota protects against very large or deep input.
///
/// # Errors
///
/// Returns an error for an invalid or unsupported header, an invalid schema,
/// malformed or truncated payloads, a target incompatible with the wire types,
/// a custom `Deserialize` failure, an I/O failure, or trailing bytes. Failed
/// reads do not restore the reader's original position.
///
/// # Examples
///
/// ```
/// let bytes = asonb::ser_to_bytes(&vec![1i32, 2])?;
/// let values: Vec<i32> = asonb::de_from_reader(bytes.as_slice())?;
/// assert_eq!(values, [1, 2]);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn de_from_reader<T: de::DeserializeOwned, R: Read>(reader: R) -> Result<T, AsonbError> {
    read_document(&mut ReaderInput::new(reader))
}

/// Reads exactly one document from `bytes`, allowing `T` to borrow its payloads.
///
/// Borrowed results cannot outlive `bytes`. Regular and Long strings/byte data
/// are contiguous and can be lent directly; MultiChunk payloads are joined into
/// owned storage and therefore cannot satisfy a borrowed `&str`/byte target.
/// Numeric values are decoded safely without alignment assumptions. The target
/// needs only Serde `Deserialize`, not [`crate::Schema`]. Borrowing is available
/// only when the target's `Deserialize` implementation requests it; asking for a
/// `String` or owned byte buffer still allocates. The schema and container
/// storage are not zero-copy. There are no size or depth quotas.
///
/// # Errors
///
/// Returns an error for an invalid header or schema, malformed or truncated
/// payloads (including invalid UTF-8 or Unicode scalars), incompatible target
/// types, custom deserialization failures, or bytes after the root value.
/// A borrowed string or byte target also fails for a MultiChunk payload, even
/// when that payload has only one chunk.
///
/// # Examples
///
/// ```
/// let bytes = asonb::ser_to_bytes(&"hello")?;
/// let text: &str = asonb::de_from_slice(&bytes)?;
/// assert_eq!(text, "hello");
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn de_from_slice<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, AsonbError> {
    read_document(&mut SliceInput::new(bytes))
}

/// Reads one finite document into its schema and an entirely owned value tree.
///
/// `reader` starts at the document header and is consumed by this call. The
/// result is `(schema, root_value)`, with all text, bytes, and containers owned;
/// neither a Rust model nor a [`crate::Schema`] implementation is needed.
/// All payloads, including MultiChunk Lists, are materialized in memory.
///
/// The EOF check may block on a live source and is not suitable for concatenated
/// documents. Use [`value_document_reader`] to stream supported List roots.
/// No library-imposed resource quota limits materialization.
///
/// # Errors
///
/// Returns an error for an invalid header or schema, malformed or truncated
/// payloads, platform-capacity or fallible buffer-allocation failures, I/O
/// failures, or trailing bytes. Reading is not rolled back on failure.
///
/// # Examples
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
    read_value_document(&mut ReaderInput::new(reader))
}

/// Reads one complete document, borrowing contiguous string and byte payloads.
///
/// `bytes` must contain exactly one document, including its header. Returned
/// `(schema, root_value)` borrows remain valid only while this slice lives.
/// Regular and Long string/byte leaves borrow their payloads. MultiChunk text
/// and byte data are joined into owned storage, even for a single chunk;
/// schemas and container vectors are also owned. Numeric values are copied
/// from little-endian bytes, never viewed through unaligned native references.
/// This API materializes entire containers rather than iterating over them.
///
/// # Errors
///
/// Returns an error for an invalid header or schema, malformed or truncated
/// payloads, platform-capacity or fallible buffer-allocation failures, or
/// trailing bytes. Each String chunk must be valid UTF-8 independently.
/// There are no library-imposed size or nesting quotas.
///
/// # Examples
///
/// ```
/// use std::borrow::Cow;
/// let bytes = asonb::ser_to_bytes(&"hello")?;
/// let (_, value) = asonb::de_value_from_slice(&bytes)?;
/// assert!(matches!(value, asonb::Value::String(Cow::Borrowed("hello"))));
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn de_value_from_slice(bytes: &[u8]) -> Result<(DocumentSchema, Value<'_>), AsonbError> {
    read_value_document(&mut SliceInput::new(bytes))
}

/// Shares header/schema/value/EOF checks for the dynamic [`Value`] readers.
fn read_value_document<'de>(
    input: &mut impl Input<'de>,
) -> Result<(DocumentSchema, Value<'de>), AsonbError> {
    input.header()?;
    let schema = DocumentSchema::read(input)?;
    let value = read_value(input, &schema, &schema.root, true)?;
    input.finish()?;
    Ok((schema, value))
}

/// Opens a MultiChunk root List and returns an iterator of owned `T` elements.
///
/// `reader` is taken by value; pass `&mut reader` to retain access after dropping
/// the iterator. Construction reads and validates the header/schema, but no
/// elements or chunk counts. The root may be a schema reference resolving to a
/// MultiChunk List. `T` needs only `DeserializeOwned`, not [`crate::Schema`].
///
/// The iterator yields `Result<T, AsonbError>` one element at a time, independent
/// of chunk boundaries. It stops after the marker, EOF between complete root
/// chunks, or its first error. It never checks for trailing bytes beyond an
/// explicit marker. Dropping it early does not drain or validate the remainder;
/// consume it to exhaustion to check the entire List.
///
/// # Errors
///
/// Construction fails for an invalid header or schema, a root other than a
/// MultiChunk List, or an I/O error while opening the document. Element type
/// mismatches, malformed chunk counts, truncated payloads, and subsequent I/O
/// errors are returned during iteration, not by this constructor.
///
/// # Examples
///
/// ```
/// let mut bytes = Vec::new();
/// let mut writer = asonb::list_stream_writer::<i32, _>(&mut bytes);
/// writer.start_list()?;
/// writer.writes(&[10, 20])?;
/// writer.end_list()?;
/// let values = asonb::list_stream_reader::<i32, _>(bytes.as_slice())?
///     .collect::<Result<Vec<_>, _>>()?;
/// assert_eq!(values, [10, 20]);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn list_stream_reader<T: de::DeserializeOwned, R: Read>(
    reader: R,
) -> Result<ListStreamReader<T, R>, AsonbError> {
    let mut input = ReaderInput::new(reader);
    input.header()?;
    let schema = DocumentSchema::read(&mut input)?;
    let Type::List(Encoding::MultiChunk, item) = schema.resolve(schema.root.clone())? else {
        return Err(AsonbError::Message(
            "List streaming requires a multi-chunk root List.".into(),
        ));
    };
    Ok(ListStreamReader {
        input,
        schema,
        item: *item,
        remaining: 0,
        finished: false,
        root: true,
        marker: PhantomData,
    })
}

/// Opens a Result containing a streamed MultiChunk List or an owned business error.
///
/// `T` is the List element type and `E` is the error payload type. Construction
/// reads the header, schema and variant index, but no successful List elements.
/// An Err payload is decoded in full before returning. Neither branch checks EOF
/// or trailing bytes. Unlike a root List, the nested List requires a zero-count
/// terminator; consume the iterator to its end to validate the complete payload.
/// Neither target type needs [`crate::Schema`].
///
/// The reader is taken by value; pass `&mut reader` to continue reading a later
/// document once this result has been consumed or dropped. A successfully
/// decoded business error is [`ResultStreamReader::Err`], not the outer
/// `Result::Err(AsonbError)`. Its `E` value is fully owned.
///
/// # Errors
///
/// Returns an error for an invalid header or schema, a root that is not the
/// predefined `Result` enumeration, an invalid variant index, a selected `Ok`
/// payload that is not a MultiChunk List, an incompatible or malformed `Err`
/// payload, or an I/O or custom deserialization failure during construction.
/// Successful List element errors and a missing List end marker are deferred
/// to the returned iterator.
///
/// # Examples
///
/// ```
/// let mut bytes = Vec::new();
/// let mut writer = asonb::result_stream_writer::<i32, String, _>(&mut bytes).start_ok()?;
/// writer.writes(&[10, 20])?;
/// writer.end_list()?;
/// match asonb::result_stream_reader::<i32, String, _>(bytes.as_slice())? {
///     asonb::ResultStreamReader::Ok(stream) => {
///         assert_eq!(stream.collect::<Result<Vec<_>, _>>()?, [10, 20]);
///     }
///     asonb::ResultStreamReader::Err(error) => panic!("{error}"),
/// }
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn result_stream_reader<T: de::DeserializeOwned, E: de::DeserializeOwned, R: Read>(
    reader: R,
) -> Result<ResultStreamReader<T, E, R>, AsonbError> {
    let mut input = ReaderInput::new(reader);
    input.header()?;
    let schema = DocumentSchema::read(&mut input)?;
    let root = schema.resolve(schema.root.clone())?;
    if !matches!(&root, Type::Enum(name, _) if name == "Result") {
        return Err(AsonbError::Message(
            "Result streaming requires a root Result enumeration.".into(),
        ));
    }
    let (_, variant) = Deserializer::new(&mut input, &schema, root, true).enumeration()?;
    match variant {
        Variant::Newtype(name, ty) if name == "Ok" => {
            let Type::List(Encoding::MultiChunk, item) = schema.resolve(ty)? else {
                return Err(AsonbError::Message(
                    "Result streaming requires a multi-chunk List in Ok.".into(),
                ));
            };
            Ok(ResultStreamReader::Ok(ListStreamReader {
                input,
                schema,
                item: *item,
                remaining: 0,
                finished: false,
                root: false,
                marker: PhantomData,
            }))
        }
        Variant::Newtype(name, ty) if name == "Err" => {
            E::deserialize(&mut Deserializer::new(&mut input, &schema, ty, false))
                .map(ResultStreamReader::Err)
        }
        _ => Err(AsonbError::MessageWithOffset(
            "Expected a single-value Result::Ok or Result::Err variant.".into(),
            input.offset() - 4,
        )),
    }
}

/// The decoded branch of a root `Result` opened by [`result_stream_reader`].
///
/// This is distinct from the constructor's outer `Result<_, AsonbError>`:
/// these variants are application data, while the outer error reports invalid
/// documents or I/O failures. `T` is one successful List element, `E` is the
/// complete business-error type, and `R` is the owned or borrowed input reader.
///
/// Both branches stop at their payload boundary without requiring EOF, allowing
/// a following document to remain unread. The `Ok` iterator must still be
/// consumed to exhaustion to validate its required nested List terminator.
pub enum ResultStreamReader<T, E, R: Read> {
    /// A lazily decoded successful List, with errors reported per element.
    Ok(
        /// The open List iterator; it has not read any element yet.
        ListStreamReader<T, R>,
    ),
    /// A business error decoded completely before the constructor returned.
    Err(
        /// The owned application error, not a document-decoding error.
        E,
    ),
}

/// Opens a document for dynamic inspection, streaming MultiChunk List payloads.
///
/// `reader` starts at the header. For a MultiChunk root List, the returned
/// iterator decodes one owned element at a time and does not probe beyond its
/// marker. For a root `Result` whose selected `Ok` payload is a MultiChunk List,
/// construction consumes the variant index and the iterator yields the List
/// elements in the same way; like [`result_stream_reader`], that nested List
/// requires an explicit end marker. Every other root, including regular Lists
/// and `Result::Err`, is fully read and EOF-checked during construction;
/// iteration then yields that root once. Thus construction may block on a live
/// nonstreaming source. All yielded values are owned `Value<'static>` values;
/// no Rust model or [`crate::Schema`] implementation is required.
///
/// Use [`ValueDocumentReader::schema`] and [`ValueDocumentReader::root_type`]
/// to inspect metadata before iteration. In either streaming mode, iteration
/// yields bare List elements, not a List or a Result wrapper.
/// [`ValueDocumentReader::item_type`] describes those elements.
///
/// Pass `&mut reader` to retain the source. Only streaming modes leave bytes
/// following an explicit terminator unread; a nonstreaming root performs an
/// EOF check. Dropping a streaming reader early does not consume the rest.
///
/// # Errors
///
/// Construction fails for an invalid header or schema, I/O errors while
/// opening the document, or any malformed payload or trailing bytes in
/// nonstreaming mode. For streamed payloads, malformed chunks, truncated
/// elements, capacity/allocation errors, and I/O failures are yielded at the
/// point they occur; the iterator is exhausted after its first error.
///
/// # Examples
///
/// ```
/// let bytes = asonb::ser_to_bytes(&42i32)?;
/// let mut reader = asonb::value_document_reader(bytes.as_slice())?;
/// assert!(!reader.is_streaming_list());
/// assert_eq!(reader.next().transpose()?, Some(asonb::Value::I32(42)));
/// assert!(reader.next().is_none());
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn value_document_reader<R: Read>(reader: R) -> Result<ValueDocumentReader<R>, AsonbError> {
    let mut input = ReaderInput::new(reader);
    input.header()?;
    let schema = DocumentSchema::read(&mut input)?;
    let root = schema.resolve(schema.root.clone())?;
    let (item, list_root, pending) = match &root {
        Type::List(Encoding::MultiChunk, item) => (Some((**item).clone()), true, None),
        Type::Enum(name, _) if name == "Result" => {
            let (index, variant) =
                Deserializer::new(&mut input, &schema, root.clone(), true).enumeration()?;
            let payload = payload_type(&variant);
            match (&variant, schema.resolve(payload.clone())?) {
                (Variant::Newtype(name, _), Type::List(Encoding::MultiChunk, item))
                    if name == "Ok" =>
                {
                    (Some(*item), false, None)
                }
                _ => {
                    let payload = read_variant_value(&mut input, &schema, &variant)?;
                    input.finish()?;
                    (None, false, Some(Value::Enumeration { index, payload }))
                }
            }
        }
        _ => {
            let value = read_value(&mut input, &schema, &root, true)?;
            input.finish()?;
            (None, true, Some(value))
        }
    };
    Ok(ValueDocumentReader {
        input,
        schema,
        root,
        item,
        list_root,
        pending,
        remaining: 0,
        finished: false,
    })
}

/// Performs the common header/schema/typed-root/EOF sequence for finite inputs.
fn read_document<'de, T: Deserialize<'de>, I: Input<'de>>(input: &mut I) -> Result<T, AsonbError> {
    input.header()?;
    let schema = DocumentSchema::read(input)?;
    let value = T::deserialize(&mut Deserializer::new(
        input,
        &schema,
        schema.root.clone(),
        true,
    ))?;
    input.finish()?;
    Ok(value)
}

/// A temporary view of one expected type at the shared document input cursor.
struct Deserializer<'a, 'de, I: Input<'de>> {
    /// Input source, including its byte offset and borrowing capabilities.
    input: &'a mut I,
    /// Validated writer-provided definitions, shared by child deserializers.
    schema: &'a DocumentSchema,
    /// Expected type for this value, possibly a reference alias.
    ty: Type,
    /// Whether this value can terminate MultiChunk framing at EOF.
    root: bool,
    /// Carries the source payload lifetime independently of the cursor borrow.
    marker: PhantomData<&'de ()>,
}

impl<'a, 'de, I: Input<'de>> Deserializer<'a, 'de, I> {
    /// Creates a child view without reading bytes or copying the document.
    fn new(input: &'a mut I, schema: &'a DocumentSchema, ty: Type, root: bool) -> Self {
        Self {
            input,
            schema,
            ty,
            root,
            marker: PhantomData,
        }
    }
    /// Resolves schema aliases before matching a Serde callback to a wire class.
    fn ty(&self) -> Result<Type, AsonbError> {
        self.schema.resolve(self.ty.clone())
    }
    /// Attaches the current byte position to a target/schema mismatch.
    fn mismatch(&self, expected: &str) -> AsonbError {
        AsonbError::MessageWithOffset(
            format!("Expected {expected}, found {:?}.", self.ty),
            self.input.offset(),
        )
    }
    /// Delegates scalar parsing and validation to the dynamic value layer.
    fn value(&mut self) -> Result<Value<'de>, AsonbError> {
        read_value(self.input, self.schema, &self.ty, self.root)
    }
    /// Reads a wire index and obtains its named, shape-specific schema variant.
    fn enumeration(&mut self) -> Result<(u32, Variant), AsonbError> {
        let Type::Enum(_, variants) = self.ty()? else {
            return Err(self.mismatch("an Enumeration"));
        };
        let index = self.input.u32()?;
        let variant = variants.get(index as usize).cloned().ok_or_else(|| {
            AsonbError::MessageWithOffset(
                "Variant index out of range.".into(),
                self.input.offset() - 4,
            )
        })?;
        Ok((index, variant))
    }
    /// Presents a homogeneous List or exact-arity Tuple to a Serde visitor.
    fn sequence<V: Visitor<'de>>(
        &mut self,
        visitor: V,
        length: Option<usize>,
    ) -> Result<V::Value, AsonbError> {
        let mode = match self.ty()? {
            Type::List(encoding, item) if length.is_none() => {
                let remaining = if encoding == Encoding::MultiChunk {
                    0
                } else {
                    read_length(self.input, &encoding)?
                };
                SequenceMode::List {
                    encoding,
                    item: *item,
                    remaining,
                    ended: false,
                }
            }
            Type::Tuple(types) if length == Some(types.len()) => SequenceMode::Tuple(types, 0),
            _ => return Err(self.mismatch("a matching List or Tuple")),
        };
        let mut access = SeqAccessor {
            deserializer: self,
            mode,
        };
        let value = visitor.visit_seq(&mut access)?;
        // A successful visitor is not enough: leaving unread elements would
        // make the next sibling start at the wrong byte, so reject early exits.
        if !access.complete() {
            return Err(AsonbError::Message(
                "Visitor did not consume the complete sequence.".into(),
            ));
        }
        Ok(value)
    }
    /// Presents schema-named Object fields or payload-encoded Map pairs.
    fn mapping<V: Visitor<'de>>(
        &mut self,
        visitor: V,
        object: bool,
    ) -> Result<V::Value, AsonbError> {
        let mode = match self.ty()? {
            Type::Object(fields) if object => MappingMode::Object(fields),
            Type::Map(long, key, value) if !object => {
                let count = read_length(
                    self.input,
                    &if long {
                        Encoding::Long
                    } else {
                        Encoding::Regular
                    },
                )?;
                MappingMode::Map {
                    key: *key,
                    value: *value,
                    count,
                }
            }
            _ => return Err(self.mismatch("a matching Object or Map")),
        };
        let mut access = MapAccessor {
            deserializer: self,
            mode,
            index: 0,
            pending: None,
        };
        let value = visitor.visit_map(&mut access)?;
        if access.pending.is_some() || access.index != access.length() {
            return Err(AsonbError::Message(
                "Visitor did not consume the complete map.".into(),
            ));
        }
        Ok(value)
    }
}

/// Generates exact-type scalar visitors without implicit numeric conversions.
macro_rules! deserialize_numbers {($($method:ident,$variant:ident,$visit:ident,$ty:expr);* $(;)?)=>{$(
    /// Requires the matching wire class, then delivers its decoded scalar.
    fn $method<V:Visitor<'de>>(self,visitor:V)->Result<V::Value,AsonbError>{if self.ty()? != $ty{return Err(self.mismatch(stringify!($variant)));}let Value::$variant(value)=self.value()? else{unreachable!()};visitor.$visit(value)}
)*};}

impl<'de, I: Input<'de>> de::Deserializer<'de> for &mut Deserializer<'_, 'de, I> {
    type Error = AsonbError;
    /// Rejects untyped Serde dispatch; callers must request a concrete Rust type.
    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, AsonbError> {
        Err(self.mismatch("a concrete Deserialize target (deserialize_any is unsupported)"))
    }
    deserialize_numbers!(deserialize_i8,I8,visit_i8,Type::I8;deserialize_i16,I16,visit_i16,Type::I16;deserialize_i32,I32,visit_i32,Type::I32;deserialize_i64,I64,visit_i64,Type::I64;deserialize_u8,U8,visit_u8,Type::U8;deserialize_u16,U16,visit_u16,Type::U16;deserialize_u32,U32,visit_u32,Type::U32;deserialize_u64,U64,visit_u64,Type::U64;deserialize_f32,F32,visit_f32,Type::F32;deserialize_f64,F64,visit_f64,Type::F64;deserialize_bool,Bool,visit_bool,Type::Bool;deserialize_char,Char,visit_char,Type::Char);
    /// Lends contiguous text, or hands over owned text when borrowing is impossible.
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let Type::String(encoding) = self.ty()? else {
            return Err(self.mismatch("a String"));
        };
        match read_blob(self.input, &encoding, true, self.root)? {
            Cow::Borrowed(bytes) => visitor.visit_borrowed_str(
                std::str::from_utf8(bytes)
                    .map_err(|error| AsonbError::Message(error.to_string()))?,
            ),
            Cow::Owned(bytes) => visitor.visit_string(
                String::from_utf8(bytes).map_err(|error| AsonbError::Message(error.to_string()))?,
            ),
        }
    }
    /// Explicitly satisfies an owned String request, copying slice data if needed.
    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let Type::String(encoding) = self.ty()? else {
            return Err(self.mismatch("a String"));
        };
        let bytes = read_blob(self.input, &encoding, true, self.root)?;
        visitor.visit_string(
            String::from_utf8(bytes.into_owned())
                .map_err(|error| AsonbError::Message(error.to_string()))?,
        )
    }
    /// Lends Byte Data only when the input owns stable contiguous source bytes.
    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let Type::Bytes(encoding) = self.ty()? else {
            return Err(self.mismatch("Byte Data"));
        };
        match read_blob(self.input, &encoding, false, self.root)? {
            Cow::Borrowed(bytes) => visitor.visit_borrowed_bytes(bytes),
            Cow::Owned(bytes) => visitor.visit_byte_buf(bytes),
        }
    }
    /// Produces owned Byte Data without changing it into a List<u8>.
    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let Type::Bytes(encoding) = self.ty()? else {
            return Err(self.mismatch("Byte Data"));
        };
        let bytes = read_blob(self.input, &encoding, false, self.root)?;
        visitor.visit_byte_buf(bytes.into_owned())
    }
    /// Recognizes Option by variant name and shape rather than a fixed index.
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        let (_, variant) = self.enumeration()?;
        match variant {
            Variant::Unit(name) if name == "None" => visitor.visit_none(),
            Variant::Newtype(name, ty) if name == "Some" => {
                visitor.visit_some(&mut Deserializer::new(self.input, self.schema, ty, false))
            }
            _ => Err(self.mismatch("None or Some")),
        }
    }
    /// Rejects standalone unit instead of interpreting it as an empty document.
    fn deserialize_unit<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, AsonbError> {
        Err(AsonbError::Message("Unit is unsupported.".into()))
    }
    /// Rejects unit structs, which have no supported ASONB mapping.
    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value, AsonbError> {
        Err(AsonbError::Message("Unit structs are unsupported.".into()))
    }
    /// Adapts only the private DateTime marker into its one-i64 Serde tuple.
    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        if name == crate::date_time::MARKER {
            if self.ty()? != Type::DateTime {
                return Err(self.mismatch("DateTime"));
            }
            // This changes only the temporary Serde view. The document still
            // contains a DateTime scalar and exactly eight timestamp bytes.
            self.ty = Type::Tuple(vec![Type::I64]);
        } else {
            return Err(AsonbError::Message(
                "Newtype structs are unsupported.".into(),
            ));
        }
        visitor.visit_newtype_struct(self)
    }
    /// Requests a homogeneous List without a compile-time length requirement.
    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        self.sequence(visitor, None)
    }
    /// Requires the target tuple's length to match the schema exactly.
    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        self.sequence(visitor, Some(length))
    }
    /// Rejects tuple structs rather than flattening them implicitly.
    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _length: usize,
        _visitor: V,
    ) -> Result<V::Value, AsonbError> {
        Err(AsonbError::Message("Tuple structs are unsupported.".into()))
    }
    /// Requests actual Map keys from the payload, not Object field names.
    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        self.mapping(visitor, false)
    }
    /// Supplies writer-schema field names so Serde can reorder or ignore fields.
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        self.mapping(visitor, true)
    }
    /// Converts the selected schema variant into Serde's enum/variant access API.
    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        let (_, variant) = self.enumeration()?;
        visitor.visit_enum(VariantAccessor {
            deserializer: self,
            variant,
        })
    }
    /// Uses String decoding for an explicitly encoded identifier value.
    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        self.deserialize_str(visitor)
    }
    /// Skips an unwanted field while still validating every nested payload.
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, AsonbError> {
        skip_value(self.input, self.schema, &self.ty)?;
        visitor.visit_unit()
    }
    /// Selects binary behavior in Serde adapters that distinguish text formats.
    fn is_human_readable(&self) -> bool {
        false
    }
}

/// State needed to offer either repeated element types or fixed tuple positions.
enum SequenceMode {
    /// List framing plus progress through the current chunk or complete payload.
    List {
        /// Selects regular, long, or repeated-chunk count framing.
        encoding: Encoding,
        /// The type used for each element.
        item: Type,
        /// Elements left in the current chunk (or the entire unchunked List).
        remaining: u64,
        /// Whether a MultiChunk end marker or legal root EOF has been observed.
        ended: bool,
    },
    /// Fixed position types and the index of the next position to visit.
    Tuple(
        /// Ordered position schemas.
        Vec<Type>,
        /// Next position index.
        usize,
    ),
}
/// Bridges the shared input cursor to Serde's pull-based sequence visitor.
struct SeqAccessor<'a, 'w, 'de, I: Input<'de>> {
    /// Parent view, used to create short-lived child deserializers.
    deserializer: &'a mut Deserializer<'w, 'de, I>,
    /// Current position/count and the schema for the next child.
    mode: SequenceMode,
}
impl<'de, I: Input<'de>> SeqAccessor<'_, '_, 'de, I> {
    /// Requires both zero remaining elements and a consumed chunk terminator.
    fn complete(&self) -> bool {
        match &self.mode {
            SequenceMode::Tuple(types, index) => *index == types.len(),
            SequenceMode::List {
                encoding,
                remaining,
                ended,
                ..
            } => *remaining == 0 && (*encoding != Encoding::MultiChunk || *ended),
        }
    }
}
impl<'de, I: Input<'de>> SeqAccess<'de> for SeqAccessor<'_, '_, 'de, I> {
    type Error = AsonbError;
    /// Advances to a chunk when needed, then decodes one non-root child.
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, AsonbError> {
        let ty = match &mut self.mode {
            SequenceMode::Tuple(types, index) => {
                let Some(ty) = types.get(*index).cloned() else {
                    return Ok(None);
                };
                *index += 1;
                ty
            }
            SequenceMode::List {
                encoding,
                item,
                remaining,
                ended,
            } => {
                if *remaining == 0 {
                    if *encoding != Encoding::MultiChunk || *ended {
                        return Ok(None);
                    }
                    *remaining = self.deserializer.input.chunk(self.deserializer.root)? as u64;
                    if *remaining == 0 {
                        *ended = true;
                        return Ok(None);
                    }
                }
                *remaining -= 1;
                item.clone()
            }
        };
        seed.deserialize(&mut Deserializer::new(
            self.deserializer.input,
            self.deserializer.schema,
            ty,
            false,
        ))
        .map(Some)
    }
}

/// Distinguishes schema-supplied Object names from encoded Map keys.
enum MappingMode {
    /// Names and child schemas supplied by the document metadata.
    Object(
        /// Fields in wire order.
        Vec<Field>,
    ),
    /// Repeated key/value payloads with an explicit pair count.
    Map {
        /// Schema for each encoded key.
        key: Type,
        /// Schema for the value following each key.
        value: Type,
        /// Total pairs promised by the payload prefix.
        count: u64,
    },
}
/// Tracks pair order so a visitor cannot request two keys without a value.
struct MapAccessor<'a, 'w, 'de, I: Input<'de>> {
    /// Parent cursor and document schema.
    deserializer: &'a mut Deserializer<'w, 'de, I>,
    /// Source of keys and expected total pair count.
    mode: MappingMode,
    /// Number of pairs whose value has been requested.
    index: u64,
    /// Type of the value owed for the most recently delivered key.
    pending: Option<Type>,
}
impl<'de, I: Input<'de>> MapAccessor<'_, '_, 'de, I> {
    /// Returns the promised pair/field count without reading more bytes.
    fn length(&self) -> u64 {
        match &self.mode {
            MappingMode::Object(fields) => fields.len() as u64,
            MappingMode::Map { count, .. } => *count,
        }
    }
}
impl<'de, I: Input<'de>> MapAccess<'de> for MapAccessor<'_, '_, 'de, I> {
    type Error = AsonbError;
    /// Delivers the next name/key and remembers which value must follow it.
    fn next_key_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, AsonbError> {
        if self.pending.is_some() {
            return Err(AsonbError::Message("Map value was not consumed.".into()));
        }
        if self.index >= self.length() {
            return Ok(None);
        }
        match &self.mode {
            MappingMode::Object(fields) => {
                let field = &fields[self.index as usize];
                self.pending = Some(field.ty.clone());
                // Object names live in the schema, not beside each value in
                // the payload. Giving Serde their names enables field matching.
                seed.deserialize(field.name.clone().into_deserializer())
                    .map(Some)
            }
            MappingMode::Map { key, value, .. } => {
                self.pending = Some(value.clone());
                seed.deserialize(&mut Deserializer::new(
                    self.deserializer.input,
                    self.deserializer.schema,
                    key.clone(),
                    false,
                ))
                .map(Some)
            }
        }
    }
    /// Consumes the pending value type and decodes that pair's non-root payload.
    fn next_value_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<T::Value, AsonbError> {
        let ty = self
            .pending
            .take()
            .ok_or_else(|| AsonbError::Message("Map key was not consumed.".into()))?;
        self.index += 1;
        seed.deserialize(&mut Deserializer::new(
            self.deserializer.input,
            self.deserializer.schema,
            ty,
            false,
        ))
    }
}

/// Carries a selected schema variant from name matching through payload decoding.
struct VariantAccessor<'a, 'w, 'de, I: Input<'de>> {
    /// Shared payload cursor, already positioned after the variant index.
    deserializer: &'a mut Deserializer<'w, 'de, I>,
    /// Wire name and exact payload shape selected by that index.
    variant: Variant,
}
impl<'de, I: Input<'de>> EnumAccess<'de> for VariantAccessor<'_, '_, 'de, I> {
    type Error = AsonbError;
    type Variant = Self;
    /// Gives Serde the wire name, avoiding dependence on the Rust variant order.
    fn variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<(T::Value, Self), AsonbError> {
        let value = seed.deserialize(self.variant.name().to_owned().into_deserializer())?;
        Ok((value, self))
    }
}
impl<'de, I: Input<'de>> VariantAccess<'de> for VariantAccessor<'_, '_, 'de, I> {
    type Error = AsonbError;
    /// Requires a unit schema variant; successful validation consumes no bytes.
    fn unit_variant(self) -> Result<(), AsonbError> {
        if !matches!(self.variant, Variant::Unit(_)) {
            return Err(AsonbError::Message(
                "Expected a variant without a payload.".into(),
            ));
        }
        Ok(())
    }
    /// Requires a single payload and delegates its exact type to the seed.
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<T::Value, AsonbError> {
        let Variant::Newtype(_, ty) = self.variant else {
            return Err(AsonbError::Message(
                "Expected a single-value variant.".into(),
            ));
        };
        seed.deserialize(&mut Deserializer::new(
            self.deserializer.input,
            self.deserializer.schema,
            ty,
            false,
        ))
    }
    /// Reuses tuple arity checks for a tuple-shaped variant payload.
    fn tuple_variant<V: Visitor<'de>>(
        self,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        if !matches!(self.variant, Variant::Tuple(..)) {
            return Err(AsonbError::Message("Expected a tuple-like variant.".into()));
        }
        let ty = payload_type(&self.variant);
        de::Deserializer::deserialize_tuple(
            &mut Deserializer::new(self.deserializer.input, self.deserializer.schema, ty, false),
            length,
            visitor,
        )
    }
    /// Reuses named Object field handling for a struct-shaped variant payload.
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, AsonbError> {
        if !matches!(self.variant, Variant::Struct(..)) {
            return Err(AsonbError::Message(
                "Expected an object-like variant.".into(),
            ));
        }
        let ty = payload_type(&self.variant);
        de::Deserializer::deserialize_struct(
            &mut Deserializer::new(self.deserializer.input, self.deserializer.schema, ty, false),
            "",
            fields,
            visitor,
        )
    }
}

/// An owned-element iterator over a MultiChunk List, without whole-List buffering.
///
/// Construct a root List reader with [`list_stream_reader`], or obtain the
/// nested successful List from [`ResultStreamReader::Ok`]. Each call to
/// [`Iterator::next`] returns one `Result<T, AsonbError>`; a chunk can contain
/// several elements, and a single element can itself own large containers.
///
/// Root Lists accept EOF between chunks; Result-wrapped Lists require an
/// explicit zero marker. The reader never probes beyond an explicit marker
/// for a trailing document. It implements [`FusedIterator`]: after the marker,
/// legal root EOF, or one yielded error, every subsequent call returns `None`.
/// Errors may leave the source in the middle of a value, so reading cannot
/// safely resume from that point.
///
/// `R` is retained by the iterator. Pass `&mut R` to the constructor to recover
/// access to the source after the iterator is consumed or dropped. Dropping
/// early performs no further reads; reading the last element alone does not
/// validate the following chunk boundary or terminator.
///
/// # Examples
///
/// An explicit terminator preserves a following document:
///
/// ```
/// let mut bytes = Vec::new();
/// let mut stream = asonb::list_stream_writer::<u32, _>(&mut bytes);
/// stream.start_list()?;
/// stream.writes(&[1, 2])?;
/// stream.end_list()?;
/// asonb::ser_to_writer(&true, &mut bytes)?;
///
/// let mut source = bytes.as_slice();
/// let values = asonb::list_stream_reader::<u32, _>(&mut source)?
///     .collect::<Result<Vec<_>, _>>()?;
/// assert_eq!(values, [1, 2]);
/// assert!(asonb::de_from_reader::<bool, _>(&mut source)?);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub struct ListStreamReader<T, R: Read> {
    /// Owned reader wrapper; it consumes only bytes needed for the next item.
    input: ReaderInput<R>,
    /// Validated definitions read once at stream construction.
    schema: DocumentSchema,
    /// Resolved List element type used for every yielded T.
    item: Type,
    /// Elements still unread in the current chunk.
    remaining: u64,
    /// Latches marker, clean root EOF, or the first error permanently.
    finished: bool,
    root: bool,
    /// Describes the yielded Rust type without retaining any decoded value.
    marker: PhantomData<fn() -> T>,
}

/// A schema-inspecting iterator over owned dynamic document values.
///
/// Create this with [`value_document_reader`]. Its iteration mode depends on
/// both the root schema and, for a `Result`, the selected branch:
///
/// | Document root | Yielded values | When the payload is read |
/// | --- | --- | --- |
/// | MultiChunk List | Each List element | During iteration |
/// | `Result::Ok` containing a MultiChunk List | Each successful List element | During iteration |
/// | Any other root, including `Result::Err` | One complete root value | During construction, including an EOF check |
///
/// Every item is `Result<Value<'static>, AsonbError>` with owned text and bytes.
/// In streaming modes the List and Result wrappers are not yielded; inspect
/// [`Self::root_type`] and the mode queries to distinguish them. Regular and
/// Long Lists are materialized roots, not streaming iterators.
///
/// This implements [`FusedIterator`]: after the first error or the end of the
/// selected mode, iteration remains exhausted. Metadata stays accessible after
/// exhaustion. Dropping an unread stream does not drain it or check its tail.
/// There is no library-imposed size or nesting quota on schemas or values.
pub struct ValueDocumentReader<R: Read> {
    /// Source cursor, retained for lazy MultiChunk List reads.
    input: ReaderInput<R>,
    /// Full writer-provided metadata, including unused reference entries.
    schema: DocumentSchema,
    /// Root type with top-level reference aliases resolved.
    root: Type,
    /// Element schema for streaming mode, or None for a single materialized root.
    item: Option<Type>,
    /// Whether the streamed List is the root, which also accepts EOF between
    /// chunks; a List inside `Result::Ok` requires an explicit end marker.
    list_root: bool,
    /// A nonstreaming root waiting to be returned on the first iterator call.
    pending: Option<Value<'static>>,
    /// Elements left in the current streaming chunk.
    remaining: u64,
    /// Permanent exhaustion flag, also set after yielding one decoding error.
    finished: bool,
}

impl<R: Read> ValueDocumentReader<R> {
    /// Borrows the complete document schema without advancing the iterator.
    ///
    /// The returned reference lives as long as this borrow of the reader and
    /// includes all reference entries and the original, possibly aliased root.
    /// This performs no I/O and remains available after iteration is exhausted.
    ///
    /// # Examples
    ///
    /// ```
    /// # let bytes = asonb::ser_to_bytes(&42i32)?;
    /// let reader = asonb::value_document_reader(bytes.as_slice())?;
    /// assert_eq!(reader.schema().root, asonb::Type::I32);
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn schema(&self) -> &DocumentSchema {
        &self.schema
    }

    /// Borrows the root type after resolving its top-level reference aliases.
    ///
    /// No bytes are read. Child references, if any, still refer to the table
    /// available through [`Self::schema`]. In Result streaming mode this is the
    /// root enumeration, not the nested List or its element type.
    ///
    /// # Examples
    ///
    /// ```
    /// # let bytes = asonb::ser_to_bytes(&vec![1i32])?;
    /// let reader = asonb::value_document_reader(bytes.as_slice())?;
    /// assert!(matches!(reader.root_type(), asonb::Type::List(..)));
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn root_type(&self) -> &Type {
        &self.root
    }

    /// Reports whether iteration yields root List elements rather than one root.
    ///
    /// Only a MultiChunk root List selects this mode;
    /// nested Lists and regular/long root Lists do not. This remains a mode
    /// query after exhaustion, not a check for whether more items are available.
    /// It is always `false` when [`Self::is_streaming_result`] is `true`.
    ///
    /// # Examples
    ///
    /// ```
    /// # let bytes = asonb::ser_to_bytes(&vec![1i32])?;
    /// let reader = asonb::value_document_reader(bytes.as_slice())?;
    /// assert!(!reader.is_streaming_list()); // A regular List is one whole value.
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn is_streaming_list(&self) -> bool {
        self.item.is_some() && self.list_root
    }

    /// Reports whether iteration yields the List elements inside `Result::Ok`.
    ///
    /// Only a root `Result` whose selected `Ok` payload
    /// is a MultiChunk List selects this mode; the variant index has already
    /// been consumed. `Result::Err` and other `Ok` payloads are yielded once as
    /// a whole [`Value::Enumeration`]. This flag remains unchanged after
    /// exhaustion and is mutually exclusive with [`Self::is_streaming_list`].
    ///
    /// # Examples
    ///
    /// ```
    /// let mut bytes = Vec::new();
    /// let mut writer = asonb::result_stream_writer::<i32, String, _>(&mut bytes).start_ok()?;
    /// writer.writes(&[10, 20])?;
    /// writer.end_list()?;
    /// let mut reader = asonb::value_document_reader(bytes.as_slice())?;
    /// assert!(reader.is_streaming_result());
    /// assert_eq!(reader.item_type(), Some(&asonb::Type::I32));
    /// assert_eq!(reader.next().transpose()?, Some(asonb::Value::I32(10)));
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn is_streaming_result(&self) -> bool {
        self.item.is_some() && !self.list_root
    }

    /// Borrows the element type of the streamed List, if any.
    ///
    /// Returns `None` when the reader yields a single materialized root. The
    /// type may be a reference into the table available through [`Self::schema`].
    /// It describes yielded elements in either streaming mode, performs no I/O,
    /// and remains `Some` after the stream ends.
    ///
    /// # Examples
    ///
    /// ```
    /// # let bytes = asonb::ser_to_bytes(&42i32)?;
    /// let reader = asonb::value_document_reader(bytes.as_slice())?;
    /// assert_eq!(reader.item_type(), None);
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn item_type(&self) -> Option<&Type> {
        self.item.as_ref()
    }
}

impl<R: Read> Iterator for ValueDocumentReader<R> {
    type Item = Result<Value<'static>, AsonbError>;

    /// Yields the pending root or one streamed element, latching EOF and errors.
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        if let Some(value) = self.pending.take() {
            self.finished = true;
            return Some(Ok(value));
        }
        let Some(item_type) = &self.item else {
            self.finished = true;
            return None;
        };
        let result = (|| {
            if self.remaining == 0 {
                self.remaining = self.input.chunk(self.list_root)? as u64;
                if self.remaining == 0 {
                    return Ok(None);
                }
            }
            self.remaining -= 1;
            read_value(&mut self.input, &self.schema, item_type, false).map(Some)
        })();
        match result {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => {
                self.finished = true;
                None
            }
            Err(error) => {
                self.finished = true;
                Some(Err(error))
            }
        }
    }
}

impl<T: de::DeserializeOwned, R: Read> Iterator for ListStreamReader<T, R> {
    type Item = Result<T, AsonbError>;
    /// Reads a chunk boundary only when needed, then deserializes one owned item.
    /// After an error, the cursor may be mid-value, so iteration never resumes.
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let result = (|| {
            if self.remaining == 0 {
                self.remaining = self.input.chunk(self.root)? as u64;
                if self.remaining == 0 {
                    return Ok(None);
                }
            }
            self.remaining -= 1;
            T::deserialize(&mut Deserializer::new(
                &mut self.input,
                &self.schema,
                self.item.clone(),
                false,
            ))
            .map(Some)
        })();
        match result {
            Ok(Some(value)) => Some(Ok(value)),
            Ok(None) => {
                self.finished = true;
                None
            }
            Err(error) => {
                self.finished = true;
                Some(Err(error))
            }
        }
    }
}
impl<T: de::DeserializeOwned, R: Read> FusedIterator for ListStreamReader<T, R> {}
impl<R: Read> FusedIterator for ValueDocumentReader<R> {}

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
        Type::Object(fields) => {
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
            Value::Enumeration {
                index,
                payload: read_variant_value(input, schema, variant)?,
            }
        }
        Type::Reference(_) => unreachable!(),
    })
}

/// Reads the payload of an already selected variant.
pub(crate) fn read_variant_value<'de, I: Input<'de>>(
    input: &mut I,
    schema: &DocumentSchema,
    variant: &Variant,
) -> Result<VariantValue<'de>, AsonbError> {
    let mut read_all = |types: &mut dyn Iterator<Item = &Type>| {
        let mut values = Vec::new();
        for ty in types {
            let value = read_value(input, schema, ty, false)?;
            push_value(&mut values, value)?;
        }
        Ok::<_, AsonbError>(values)
    };
    Ok(match variant {
        Variant::Unit(_) => VariantValue::Unit,
        Variant::Newtype(_, ty) => {
            VariantValue::Newtype(Box::new(read_value(input, schema, ty, false)?))
        }
        Variant::Tuple(_, types) => VariantValue::Tuple(read_all(&mut types.iter())?),
        Variant::Struct(_, fields) => {
            VariantValue::Struct(read_all(&mut fields.iter().map(|field| &field.ty))?)
        }
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
        Type::Object(fields) => {
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
    use crate::ser::{list_stream_writer, ser_to_bytes};

    #[test]
    /// Allows large valid counts while rejecting impossible platform allocations.
    fn test_platform_container_capacity() {
        assert_eq!(value_count::<Value<'_>>(0).unwrap(), 0);
        assert!(value_count::<Value<'_>>(u64::MAX).is_err());
        assert!(value_count::<Value<'_>>(1_000_001).is_ok());
    }

    #[test]
    fn test_result_stream_reader() {
        let ty = Type::Enum(
            "Result".into(),
            vec![
                Variant::Newtype("Err".into(), Type::String(Encoding::Regular)),
                Variant::Newtype(
                    "Ok".into(),
                    Type::List(Encoding::MultiChunk, Box::new(Type::I32)),
                ),
            ],
        );
        let mut payload = 1u32.to_le_bytes().to_vec();
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&11i32.to_le_bytes());
        payload.extend_from_slice(&13i32.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        let bytes = crate::test_utils::document(ty.clone(), &payload);
        let ResultStreamReader::Ok(stream) =
            result_stream_reader::<i32, String, _>(bytes.as_slice()).unwrap()
        else {
            panic!("expected a streamed List");
        };
        assert_eq!(stream.collect::<Result<Vec<_>, _>>().unwrap(), [11, 13]);

        for end in 0..bytes.len() {
            if let Ok(ResultStreamReader::Ok(mut stream)) =
                result_stream_reader::<i32, String, _>(&bytes[..end])
            {
                assert!(
                    stream.by_ref().collect::<Result<Vec<_>, _>>().is_err(),
                    "truncated at {end}"
                );
                assert!(stream.next().is_none());
            }
        }

        let bytes = crate::test_utils::document(ty, &[1, 0, 0, 0, 0, 0, 0, 0]);
        let ResultStreamReader::Ok(mut stream) =
            result_stream_reader::<i32, String, _>(bytes.as_slice()).unwrap()
        else {
            panic!("expected an empty streamed List");
        };
        assert!(stream.next().is_none());
        assert!(stream.next().is_none());

        let bytes = ser_to_bytes(&Err::<Vec<i32>, _>("failed")).unwrap();
        assert!(
            matches!(result_stream_reader::<i32, String, _>(bytes.as_slice()).unwrap(), ResultStreamReader::Err(error) if error == "failed")
        );
    }

    #[test]
    fn test_result_stream_reader_invalid_inputs() {
        for ty in [
            Type::List(Encoding::MultiChunk, Box::new(Type::I32)),
            Type::Enum(
                "Other".into(),
                vec![Variant::Newtype("Ok".into(), Type::I32)],
            ),
            Type::Enum(
                "Result".into(),
                vec![Variant::Newtype("Ok".into(), Type::I32)],
            ),
            Type::Enum(
                "Result".into(),
                vec![Variant::Newtype(
                    "Ok".into(),
                    Type::List(Encoding::Regular, Box::new(Type::I32)),
                )],
            ),
            Type::Enum("Result".into(), vec![Variant::Unit("Ok".into())]),
            Type::Enum("Result".into(), vec![Variant::Unit("Err".into())]),
            Type::Enum(
                "Result".into(),
                vec![Variant::Tuple("Ok".into(), vec![Type::I32, Type::I32])],
            ),
            Type::Enum(
                "Result".into(),
                vec![Variant::Newtype("Other".into(), Type::I32)],
            ),
        ] {
            let bytes = crate::test_utils::document(ty, &[0; 16]);
            assert!(result_stream_reader::<i32, String, _>(bytes.as_slice()).is_err());
        }
        let ty = Type::Enum(
            "Result".into(),
            vec![Variant::Newtype(
                "Ok".into(),
                Type::List(Encoding::MultiChunk, Box::new(Type::I32)),
            )],
        );
        let bytes = crate::test_utils::document(ty.clone(), &1u32.to_le_bytes());
        assert!(result_stream_reader::<i32, String, _>(bytes.as_slice()).is_err());
        let bytes = crate::test_utils::document(ty, &[]);
        assert!(result_stream_reader::<i32, String, _>(bytes.as_slice()).is_err());

        let mut bytes = Vec::new();
        let mut writer = crate::result_stream_writer::<i32, String, _>(&mut bytes)
            .start_ok()
            .unwrap();
        writer.write(&11).unwrap();
        writer.end_list().unwrap();
        let ResultStreamReader::Ok(mut stream) =
            result_stream_reader::<String, String, _>(bytes.as_slice()).unwrap()
        else {
            panic!("expected a streamed List")
        };
        assert!(stream.next().unwrap().is_err());
        assert!(stream.next().is_none());
        let bytes = ser_to_bytes(&Err::<Vec<i32>, _>(42i32)).unwrap();
        assert!(result_stream_reader::<i32, String, _>(bytes.as_slice()).is_err());
    }

    #[test]
    fn test_result_stream_reader_io_and_boundaries() {
        use crate::test_utils::FaultReader;
        let mut success = Vec::new();
        let mut writer = crate::result_stream_writer::<i32, String, _>(&mut success)
            .start_ok()
            .unwrap();
        writer.writes(&[11, 13]).unwrap();
        writer.end_list().unwrap();
        let error = ser_to_bytes(&Err::<Vec<i32>, _>("failed")).unwrap();

        for bytes in [&success, &error] {
            for fail_at in 0..bytes.len() {
                let mut source = FaultReader::new(bytes);
                source.fail_at = fail_at;
                let error = match result_stream_reader::<i32, String, _>(source) {
                    Err(error) => error,
                    Ok(ResultStreamReader::Ok(mut stream)) => {
                        let error = stream.by_ref().collect::<Result<Vec<_>, _>>().unwrap_err();
                        assert!(stream.next().is_none());
                        error
                    }
                    Ok(ResultStreamReader::Err(_)) => {
                        panic!("expected a read failure at {fail_at}")
                    }
                };
                assert!(error.to_string().contains("injected read failure"));
            }

            let mut source = FaultReader::new(bytes);
            source.fail_at = bytes.len();
            match result_stream_reader::<i32, String, _>(&mut source).unwrap() {
                ResultStreamReader::Ok(stream) => {
                    assert_eq!(stream.collect::<Result<Vec<_>, _>>().unwrap(), [11, 13])
                }
                ResultStreamReader::Err(error) => assert_eq!(error, "failed"),
            }
            assert_eq!(source.position, bytes.len());

            let mut suffixed = bytes.clone();
            suffixed.extend_from_slice(b"following document");
            let mut source = suffixed.as_slice();
            if let ResultStreamReader::Ok(stream) =
                result_stream_reader::<i32, String, _>(&mut source).unwrap()
            {
                assert_eq!(stream.collect::<Result<Vec<_>, _>>().unwrap(), [11, 13]);
            }
            assert_eq!(source, b"following document");
        }

        let mut source = FaultReader::new(&success);
        let boundary = success.len() - 16;
        {
            let ResultStreamReader::Ok(stream) =
                result_stream_reader::<i32, String, _>(&mut source).unwrap()
            else {
                panic!("expected a streamed List")
            };
            drop(stream);
        }
        assert_eq!(source.position, boundary);
    }

    #[test]
    /// Distinguishes lazy MultiChunk List elements from a single materialized root.
    fn test_dynamic_value_document_reader_streams_only_multichunk_lists() {
        let list_schema =
            DocumentSchema::new(Type::List(Encoding::MultiChunk, Box::new(Type::I32)));
        let list_value = Value::List(vec![Value::I32(1), Value::I32(2)]);
        let mut bytes = Vec::new();
        crate::ser_value_to_writer(&list_schema, &list_value, &mut bytes).unwrap();
        let mut reader = value_document_reader(bytes.as_slice()).unwrap();
        assert!(reader.is_streaming_list());
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(1));
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(2));
        assert!(reader.next().is_none());

        let scalar_schema = DocumentSchema::new(Type::I32);
        let scalar_value = Value::I32(9);
        let mut bytes = Vec::new();
        crate::ser_value_to_writer(&scalar_schema, &scalar_value, &mut bytes).unwrap();
        let mut reader = value_document_reader(bytes.as_slice()).unwrap();
        assert!(!reader.is_streaming_list());
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(9));
        assert!(reader.next().is_none());
    }

    #[test]
    /// Streams `Result::Ok` MultiChunk elements and materializes other Result roots.
    fn test_dynamic_value_document_reader_streams_result_ok_lists() {
        let mut bytes = Vec::new();
        let mut writer = crate::result_stream_writer::<i32, String, _>(&mut bytes)
            .start_ok()
            .unwrap();
        writer.writes(&[11, 13]).unwrap();
        writer.end_list().unwrap();
        let mut reader = value_document_reader(bytes.as_slice()).unwrap();
        assert!(reader.is_streaming_result());
        assert!(!reader.is_streaming_list());
        assert_eq!(reader.item_type(), Some(&Type::I32));
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(11));
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(13));
        assert!(reader.next().is_none());

        // The nested List must end with its marker; EOF at a chunk boundary is truncation.
        let truncated = &bytes[..bytes.len() - 4];
        let mut reader = value_document_reader(truncated).unwrap();
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(11));
        assert_eq!(reader.next().unwrap().unwrap(), Value::I32(13));
        assert!(reader.next().unwrap().is_err());
        assert!(reader.next().is_none());

        let mut bytes = Vec::new();
        crate::result_stream_writer::<i32, String, _>(&mut bytes)
            .write_err(&"failed".to_owned())
            .unwrap();
        let mut reader = value_document_reader(bytes.as_slice()).unwrap();
        assert!(!reader.is_streaming_result());
        assert_eq!(reader.item_type(), None);
        let Value::Enumeration { index, payload } = reader.next().unwrap().unwrap() else {
            panic!("expected a Result value");
        };
        assert_eq!(
            payload,
            VariantValue::Newtype(Box::new(Value::String("failed".into())))
        );
        assert!(
            matches!(&reader.root_type(), Type::Enum(_, variants) if variants[index as usize].name() == "Err")
        );
        assert!(reader.next().is_none());

        let bytes = ser_to_bytes(&Ok::<Vec<i32>, String>(vec![7])).unwrap();
        let mut reader = value_document_reader(bytes.as_slice()).unwrap();
        assert!(!reader.is_streaming_result());
        assert!(matches!(
            reader.next().unwrap().unwrap(),
            Value::Enumeration { payload: VariantValue::Newtype(payload), .. }
                if *payload == Value::List(vec![Value::I32(7)])
        ));
        assert!(reader.next().is_none());
    }

    #[test]
    /// Accepts all nonzero Boolean bytes, but rejects width mismatches and suffixes.
    fn test_boolean_and_type_errors() {
        for byte in 0u8..=255 {
            let mut bytes = ser_to_bytes(&false).unwrap();
            bytes[13] = byte;
            assert_eq!(de_from_slice::<bool>(&bytes).unwrap(), byte != 0);
        }
        assert!(de_from_slice::<i64>(&ser_to_bytes(&1i32).unwrap()).is_err());
        let mut bytes = ser_to_bytes(&1i32).unwrap();
        bytes.push(0);
        assert!(de_from_slice::<i32>(&bytes).is_err());
    }
    #[test]
    /// Checks actual pointer reuse for slice text and owned decoding from readers.
    fn test_borrowed_strings() {
        let bytes = ser_to_bytes(&"hello").unwrap();
        let value = de_from_slice::<&str>(&bytes).unwrap();
        assert_eq!(value, "hello");
        assert_eq!(value.as_ptr(), bytes[17..].as_ptr());
        let value = de_from_reader::<String, _>(bytes.as_slice()).unwrap();
        assert_eq!(value, "hello");
    }
    #[test]
    /// Checks permanent exhaustion after a marker and after a truncated element.
    fn test_stream_end_and_error() {
        let mut bytes = Vec::new();
        let mut writer = list_stream_writer::<i32, _>(&mut bytes);
        writer.start_list().unwrap();
        writer.write(&11).unwrap();
        writer.write(&13).unwrap();
        writer.end_list().unwrap();
        let mut source = bytes.as_slice();
        let mut reader = list_stream_reader::<i32, _>(&mut source).unwrap();
        assert_eq!(reader.next().unwrap().unwrap(), 11);
        assert_eq!(reader.next().unwrap().unwrap(), 13);
        assert!(reader.next().is_none());
        assert!(reader.next().is_none());
        let truncated = &bytes[..bytes.len() - 5];
        let mut reader = list_stream_reader::<i32, _>(truncated).unwrap();
        assert_eq!(reader.next().unwrap().unwrap(), 11);
        assert!(reader.next().unwrap().is_err());
        assert!(reader.next().is_none());
    }

    /// Generates both input-mode and truncation checks for each integer width.
    macro_rules! integer_test {
        ($name:ident,$rust:ty,$ty:expr) => {
            #[test]
            /// Reads this width's extrema and rejects every incomplete document.
            fn $name() {
                for value in [<$rust>::MIN, 0, <$rust>::MAX] {
                    let bytes = crate::test_utils::document($ty, &value.to_le_bytes());
                    assert_eq!(de_from_slice::<$rust>(&bytes).unwrap(), value);
                    assert_eq!(de_from_reader::<$rust, _>(bytes.as_slice()).unwrap(), value);
                    for end in 0..bytes.len() {
                        assert!(de_from_slice::<$rust>(&bytes[..end]).is_err());
                    }
                }
            }
        };
    }
    integer_test!(test_i8, i8, Type::I8);
    integer_test!(test_i16, i16, Type::I16);
    integer_test!(test_i32, i32, Type::I32);
    integer_test!(test_i64, i64, Type::I64);
    integer_test!(test_u8, u8, Type::U8);
    integer_test!(test_u16, u16, Type::U16);
    integer_test!(test_u32, u32, Type::U32);
    integer_test!(test_u64, u64, Type::U64);

    #[test]
    /// Preserves float input bits, including NaNs, and validates Unicode scalars.
    fn test_floats_and_characters() {
        for bits in [
            0u32, 0x80000000, 1, 0x7f800000, 0xff800000, 0x7fc00011, 0xffc00011,
        ] {
            let bytes = crate::test_utils::document(Type::F32, &bits.to_le_bytes());
            assert_eq!(de_from_slice::<f32>(&bytes).unwrap().to_bits(), bits);
        }
        for bits in [
            0u64,
            0x8000000000000000,
            1,
            0x7ff0000000000000,
            0xfff0000000000000,
            0x7ff8000000000011,
        ] {
            let bytes = crate::test_utils::document(Type::F64, &bits.to_le_bytes());
            assert_eq!(de_from_slice::<f64>(&bytes).unwrap().to_bits(), bits);
        }
        for scalar in [0u32, 0x42, 0x6587, 0xd7ff, 0xe000, 0x10ffff] {
            let bytes = crate::test_utils::document(Type::Char, &scalar.to_le_bytes());
            assert_eq!(de_from_slice::<char>(&bytes).unwrap() as u32, scalar);
        }
        for scalar in [0xd800u32, 0xdfff, 0x110000, u32::MAX] {
            assert!(
                de_from_slice::<char>(&crate::test_utils::document(
                    Type::Char,
                    &scalar.to_le_bytes()
                ))
                .is_err()
            );
        }
    }

    #[test]
    /// Checks zero-copy bytes, joined text, per-chunk UTF-8, and impossible lengths.
    fn test_byte_borrowing_and_chunks() {
        let bytes =
            crate::test_utils::document(Type::Bytes(Encoding::Regular), &[3, 0, 0, 0, 0xff, 0, 1]);
        let value = de_from_slice::<&serde_bytes::Bytes>(&bytes).unwrap();
        assert_eq!(value.as_ptr(), bytes[17..].as_ptr());
        assert_eq!(value.as_ref(), [0xff, 0, 1]);
        let bytes = crate::test_utils::document(
            Type::String(Encoding::MultiChunk),
            &[1, 0, 0, 0, b'a', 1, 0, 0, 0, b'b', 0, 0, 0, 0],
        );
        assert_eq!(de_from_slice::<String>(&bytes).unwrap(), "ab");
        assert!(de_from_slice::<&str>(&bytes).is_err());
        let bytes = crate::test_utils::document(
            Type::String(Encoding::MultiChunk),
            &[1, 0, 0, 0, 0xc3, 1, 0, 0, 0, 0xa9, 0, 0, 0, 0],
        );
        assert!(de_from_slice::<String>(&bytes).is_err());
        let bytes =
            crate::test_utils::document(Type::String(Encoding::Long), &u64::MAX.to_le_bytes());
        assert!(de_from_slice::<String>(&bytes).is_err());
    }

    #[test]
    /// Exercises repeated short/interrupted reads and errors at every byte offset.
    fn test_short_reads_and_io_errors() {
        let bytes = ser_to_bytes(&vec!["foo", "bar"]).unwrap();
        let reader = crate::test_utils::FaultReader::new(&bytes);
        assert_eq!(
            de_from_reader::<Vec<String>, _>(reader).unwrap(),
            ["foo", "bar"]
        );
        for fail_at in 0..bytes.len() {
            let mut reader = crate::test_utils::FaultReader::new(&bytes);
            reader.fail_at = fail_at;
            assert!(de_from_reader::<Vec<String>, _>(reader).is_err());
        }
    }

    #[test]
    /// Ensures a List marker leaves a following document untouched in the source.
    fn test_stream_does_not_read_ahead() {
        let mut bytes = Vec::new();
        let mut writer = list_stream_writer::<i32, _>(&mut bytes);
        writer.start_list().unwrap();
        writer.write(&11).unwrap();
        writer.end_list().unwrap();
        let boundary = bytes.len();
        bytes.extend_from_slice(&ser_to_bytes(&13i32).unwrap());
        let mut source = bytes.as_slice();
        {
            let mut reader = list_stream_reader::<i32, _>(&mut source).unwrap();
            assert_eq!(reader.next().unwrap().unwrap(), 11);
            assert!(reader.next().is_none());
        }
        assert_eq!(source, &bytes[boundary..]);
        assert_eq!(de_from_reader::<i32, _>(source).unwrap(), 13);
    }
}
