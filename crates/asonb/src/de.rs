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
    value::{payload_type, read_blob, read_length, read_value, skip_value},
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
/// reader cannot lend stable storage. Invalid input, type mismatches, I/O errors,
/// or trailing bytes fail the call. Checking EOF can block on a live source.
/// No library-imposed resource quota protects against very large/deep input.
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
/// needs only Serde `Deserialize`; schema errors, type mismatches, malformed
/// payloads, and trailing bytes are rejected. There are no size or depth quotas.
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

/// Opens a MultiChunk root List and returns an iterator of owned `T` elements.
///
/// `reader` is taken by value; pass `&mut reader` to retain access after dropping
/// the iterator. Construction reads and validates the header/schema, but no
/// elements. A different root type fails immediately; element type errors occur
/// during iteration. The iterator stops after the marker, root-boundary EOF, or
/// its first error. It never checks for trailing bytes beyond an explicit marker.
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
        marker: PhantomData,
    })
}

/// Opens a document for dynamic inspection, streaming only MultiChunk root Lists.
///
/// `reader` starts at the header. For a MultiChunk List, the returned iterator
/// decodes one owned element at a time and does not probe beyond its marker.
/// Every other root, including regular Lists, is fully read and EOF-checked
/// during construction; iteration then yields that root once. Thus construction
/// may block on a live nonstreaming source. Errors are returned either here or,
/// for streamed payloads, by the iterator at the point they are encountered.
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
    let (item, pending) = match &root {
        Type::List(Encoding::MultiChunk, item) => (Some((**item).clone()), None),
        _ => {
            let value = read_value(&mut input, &schema, &root, true)?;
            input.finish()?;
            (None, Some(value))
        }
    };
    Ok(ValueDocumentReader {
        input,
        schema,
        root,
        item,
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
            Type::Object(_, fields) if object => MappingMode::Object(fields),
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

/// Incremental root List reader. It is fused after its marker, EOF, or first
/// error, and never probes beyond an explicit marker for a trailing document.
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
    /// Describes the yielded Rust type without retaining any decoded value.
    marker: PhantomData<fn() -> T>,
}

/// Reads a dynamic document, streaming items only for a multi-chunk root List.
/// Other roots are materialized during construction and yielded once. Both
/// modes return owned values and remain exhausted after the first error or end.
pub struct ValueDocumentReader<R: Read> {
    /// Source cursor, retained for lazy MultiChunk List reads.
    input: ReaderInput<R>,
    /// Full writer-provided metadata, including unused reference entries.
    schema: DocumentSchema,
    /// Root type with top-level reference aliases resolved.
    root: Type,
    /// Element schema for streaming mode, or None for a single materialized root.
    item: Option<Type>,
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
    /// There are no arguments. The returned reference lives as long as this
    /// borrow of the reader and includes the original, possibly aliased root.
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
    /// There are no arguments and no bytes are read. Child references, if any,
    /// still refer to the table available through [`Self::schema`].
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
    /// There are no arguments. Only a MultiChunk root List selects this mode;
    /// nested Lists and regular/long root Lists do not. This remains a mode
    /// query after exhaustion, not a check for whether more items are available.
    ///
    /// ```
    /// # let bytes = asonb::ser_to_bytes(&vec![1i32])?;
    /// let reader = asonb::value_document_reader(bytes.as_slice())?;
    /// assert!(!reader.is_streaming_list()); // A regular List is one whole value.
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn is_streaming_list(&self) -> bool {
        self.item.is_some()
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
                self.remaining = self.input.chunk(true)? as u64;
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
                self.remaining = self.input.chunk(true)? as u64;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ser::{list_stream_writer, ser_to_bytes};

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
