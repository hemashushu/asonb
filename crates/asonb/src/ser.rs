// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Schema-guided Serde encoding and forward-only List and Result production.
//! Known-length containers write directly; unknown-length regular containers
//! buffer only their own encoded payload until their count is available.

use crate::{
    AsonbError, DocumentSchema, Encoding, Field, Schema, Type, Value, Variant,
    binary::write_length,
    binary::{HEADER, ValueBuffer, write_all, write_u32},
    schema::payload_type,
    value::VariantValue,
};
use serde::{Serialize, ser};
use std::{io::Write, marker::PhantomData};

/// Serializes one complete document without requiring a seekable destination.
///
/// `value` supplies both its Serde payload and its static [`Schema`]. `writer`
/// receives the header, schema, then payload; this function does not flush it.
/// The schema is built and validated before any bytes are emitted, even for
/// empty collections. Unsupported values and write failures return an error
/// and may leave partial output. No size or nesting quota is imposed.
///
/// ```
/// let mut output = Vec::new();
/// asonb::ser_to_writer(&vec![10i32, 20], &mut output)?;
/// assert_eq!(asonb::de_from_slice::<Vec<i32>>(&output)?, [10, 20]);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn ser_to_writer<T: Serialize + Schema + ?Sized, W: Write>(
    value: &T,
    writer: &mut W,
) -> Result<(), AsonbError> {
    let schema = DocumentSchema::of::<T>()?;
    write_all(writer, &HEADER)?;
    schema.write(writer)?;
    value.serialize(&mut Serializer::new(writer, &schema, schema.root.clone()))
}

/// Serializes `value` into a newly allocated, complete ASONB document.
///
/// `T` must describe the same representation through [`Schema`] and Serde's
/// `Serialize`. The returned bytes include the header and schema, not just the
/// payload. Schema and value errors are the same as for [`ser_to_writer`].
///
/// ```
/// let bytes = asonb::ser_to_bytes(&42u32)?;
/// assert_eq!(asonb::de_from_slice::<u32>(&bytes)?, 42);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn ser_to_bytes<T: Serialize + Schema + ?Sized>(value: &T) -> Result<Vec<u8>, AsonbError> {
    let mut bytes = Vec::new();
    ser_to_writer(value, &mut bytes)?;
    Ok(bytes)
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

/// Creates an idle writer for a MultiChunk root List of `T` values.
///
/// `writer` is borrowed until the stream writer is dropped. Construction writes
/// nothing and cannot fail: call [`ListStreamWriter::start_list`] to write the
/// header and schema, then write values and explicitly end the list. Dropping
/// the stream neither writes an end marker nor flushes the destination.
///
/// ```
/// let mut bytes = Vec::new();
/// let mut stream = asonb::list_stream_writer::<i32, _>(&mut bytes);
/// stream.start_list()?;
/// stream.write(&10)?;
/// stream.writes(&[20, 30])?;
/// stream.end_list()?;
/// assert_eq!(asonb::de_from_slice::<Vec<i32>>(&bytes)?, [10, 20, 30]);
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn list_stream_writer<T: Schema, W: Write>(writer: &mut W) -> ListStreamWriter<'_, W, T> {
    ListStreamWriter {
        writer,
        schema: None,
        item: None,
        state: StreamState::NotStarted,
        marker: PhantomData,
    }
}

/// Creates an idle writer for a Result containing a streamed List or an error.
///
/// Both `T` (the List element) and `E` need Schema because the header describes
/// both branches. Construction writes nothing. Choose one branch with `start_ok`
/// or `write_err`; either method consumes this writer. No method flushes the sink.
///
/// ```
/// let mut bytes = Vec::new();
/// let mut stream = asonb::result_stream_writer::<i32, String, _>(&mut bytes).start_ok()?;
/// stream.writes(&[10, 20])?;
/// stream.end_list()?;
/// assert_eq!(asonb::de_from_slice::<Result<Vec<i32>, String>>(&bytes)?, Ok(vec![10, 20]));
/// # Ok::<(), asonb::AsonbError>(())
/// ```
pub fn result_stream_writer<T: Schema, E: Schema, W: Write>(
    writer: &mut W,
) -> ResultStreamWriter<'_, W, T, E> {
    ResultStreamWriter {
        writer,
        marker: PhantomData,
    }
}

struct StreamListSchema<T>(PhantomData<fn() -> T>);

impl<T: Schema> Schema for StreamListSchema<T> {
    fn schema(builder: &mut crate::SchemaBuilder) -> Result<Type, AsonbError> {
        Ok(Type::List(
            Encoding::MultiChunk,
            Box::new(builder.build::<T>()?),
        ))
    }
}

/// A forward-only Result writer with mutually exclusive, consuming branches.
pub struct ResultStreamWriter<'a, W: Write, T, E> {
    writer: &'a mut W,
    marker: PhantomData<fn(&T, &E)>,
}

impl<'a, W: Write, T: Schema, E: Schema> ResultStreamWriter<'a, W, T, E> {
    /// Writes the header, schema and Ok variant index, returning an open List.
    ///
    /// Write elements directly without calling `start_list` again, then call
    /// `end_list` to emit the required zero-count terminator. Dropping the List
    /// does not finish it. Errors may leave partial output.
    pub fn start_ok(self) -> Result<ListStreamWriter<'a, W, T>, AsonbError> {
        let schema = DocumentSchema::of::<Result<StreamListSchema<T>, E>>()?;
        write_all(self.writer, &HEADER)?;
        schema.write(self.writer)?;
        let ty = Serializer::new(self.writer, &schema, schema.root.clone()).variant("Ok", 1)?;
        let Type::List(_, item) = schema.resolve(ty)? else {
            unreachable!()
        };
        Ok(ListStreamWriter {
            writer: self.writer,
            schema: Some(schema),
            item: Some(*item),
            state: StreamState::Open,
            marker: PhantomData,
        })
    }

    /// Writes a complete Err branch using its regular single-value encoding.
    ///
    /// Only `E` needs Serialize; errors may leave partial output.
    ///
    /// ```
    /// let mut bytes = Vec::new();
    /// asonb::result_stream_writer::<i32, String, _>(&mut bytes).write_err(&"failed".to_owned())?;
    /// assert_eq!(asonb::de_from_slice::<Result<Vec<i32>, String>>(&bytes)?, Err("failed".into()));
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn write_err(self, error: &E) -> Result<(), AsonbError>
    where
        E: Serialize,
    {
        let schema = DocumentSchema::of::<Result<StreamListSchema<T>, E>>()?;
        write_all(self.writer, &HEADER)?;
        schema.write(self.writer)?;
        ser::Serializer::serialize_newtype_variant(
            &mut Serializer::new(self.writer, &schema, schema.root.clone()),
            "Result",
            1,
            "Err",
            error,
        )
    }
}

/// A schema-guided Serde writer. Child serializers borrow the same output,
/// but have their own type and nesting state, so nested containers do not
/// overwrite the framing state of their parents.
pub struct Serializer<'a, W: Write> {
    /// Shared forward-only output; child serializers borrow it temporarily.
    writer: &'a mut W,
    /// The complete validated document schema, shared by every child.
    schema: &'a DocumentSchema,
    /// The expected type for this value, possibly a reference-table index.
    ty: Type,
}

impl<'a, W: Write> Serializer<'a, W> {
    /// Attaches one expected type to the existing document and output cursor.
    pub(crate) fn new(writer: &'a mut W, schema: &'a DocumentSchema, ty: Type) -> Self {
        Self { writer, schema, ty }
    }
    /// Resolves aliases before checking which Serde operation is allowed.
    fn ty(&self) -> Result<Type, AsonbError> {
        self.schema.resolve(self.ty.clone())
    }
    /// Reuses dynamic-value validation and canonical scalar encoding.
    fn scalar(&mut self, value: Value<'_>) -> Result<(), AsonbError> {
        write_value(self.writer, self.schema, &self.ty, &value)
    }
    /// Finds a variant by wire name, checks its shape, and writes its schema index.
    /// Rust's supplied index is not authoritative when schema order differs.
    fn variant(&mut self, name: &str, form: u8) -> Result<Type, AsonbError> {
        let Type::Enum(_, variants) = self.ty()? else {
            return Err(AsonbError::Message(
                "Expected an Enumeration schema.".into(),
            ));
        };
        let (index, variant) = variants
            .iter()
            .enumerate()
            .find(|(_, variant)| variant.name() == name)
            .ok_or_else(|| AsonbError::Message(format!("Unknown variant {name}.")))?;
        let actual = match variant {
            Variant::Unit(_) => 0,
            Variant::Newtype(_, _) => 1,
            Variant::Tuple(_, _) => 2,
            Variant::Struct(_, _) => 4,
        };
        if actual != form {
            return Err(AsonbError::Message(
                "Variant payload form does not match schema.".into(),
            ));
        }
        write_u32(self.writer, index as u64)?;
        Ok(payload_type(variant))
    }
}

/// The per-container interpretation used after resolving its document type.
enum Kind {
    /// Homogeneous elements with their count framing and shared element type.
    Sequence(
        /// Count framing.
        Encoding,
        /// Element schema.
        Type,
    ),
    /// Alternating keys and values; the Boolean selects u64 rather than u32 counts.
    Map(
        /// Whether to use long counts.
        bool,
        /// Key schema.
        Type,
        /// Value schema.
        Type,
    ),
    /// Positional children, each with its own type.
    Tuple(
        /// Types in serialized order.
        Vec<Type>,
    ),
    /// Named children, validated against schema order as they arrive from Serde.
    Object(
        /// Ordered field descriptions.
        Vec<Field>,
    ),
    /// The date adapter's private one-i64 tuple, not a document Tuple type.
    DateTime,
}

/// Per-container count and ordering validation. Only an unknown-length regular
/// sequence/map buffers its own payload to put the count before the bytes.
pub struct Compound<'a, 'w, W: Write> {
    /// Parent serializer, including its shared output and document references.
    serializer: &'a mut Serializer<'w, W>,
    /// Shape and child types for this container only.
    kind: Kind,
    /// Completed elements, fields, or key/value pairs; never half-written pairs.
    count: usize,
    /// Serde's promised count, or None when the producer cannot know it yet.
    expected: Option<usize>,
    /// True after a Map key has been written and before its value arrives.
    pending: bool,
    /// Deferred bytes for a regular/long container whose count is unknown.
    buffer: Option<ValueBuffer>,
}

impl<'a, 'w, W: Write> Compound<'a, 'w, W> {
    /// Checks the shape and emits a known count, or chooses a local buffer.
    fn new(
        serializer: &'a mut Serializer<'w, W>,
        ty: Type,
        length: Option<usize>,
    ) -> Result<Self, AsonbError> {
        let mut buffer = None;
        let kind = match ty {
            Type::List(encoding, item) => {
                if encoding != Encoding::MultiChunk {
                    if let Some(length) = length {
                        write_length(serializer.writer, &encoding, length as u64)?;
                    } else if matches!(encoding, Encoding::Regular | Encoding::Long) {
                        // Forward-only output cannot backpatch a count. Delay
                        // this payload, not the entire surrounding document.
                        buffer = Some(ValueBuffer { bytes: Vec::new() });
                    }
                }
                Kind::Sequence(encoding, *item)
            }
            Type::Map(long, key, value) => {
                if let Some(length) = length {
                    write_length(
                        serializer.writer,
                        &if long {
                            Encoding::Long
                        } else {
                            Encoding::Regular
                        },
                        length as u64,
                    )?;
                } else {
                    buffer = Some(ValueBuffer { bytes: Vec::new() });
                }
                Kind::Map(long, *key, *value)
            }
            Type::Tuple(types) => {
                if length != Some(types.len()) {
                    return Err(AsonbError::Message(
                        "Tuple arity does not match schema.".into(),
                    ));
                }
                Kind::Tuple(types)
            }
            Type::Object(fields) => {
                if length != Some(fields.len()) {
                    return Err(AsonbError::Message(
                        "Object field count does not match schema.".into(),
                    ));
                }
                Kind::Object(fields)
            }
            Type::DateTime if length == Some(1) => Kind::DateTime,
            _ => {
                return Err(AsonbError::Message(
                    "Container does not match schema.".into(),
                ));
            }
        };
        Ok(Self {
            serializer,
            kind,
            count: 0,
            expected: length,
            pending: false,
            buffer,
        })
    }

    /// Writes a child to this container's buffer or directly to the shared sink.
    fn child<T: Serialize + ?Sized>(&mut self, ty: Type, value: &T) -> Result<(), AsonbError> {
        let serializer = &mut self.serializer;
        let result = if let Some(buffer) = &mut self.buffer {
            value.serialize(&mut Serializer::new(buffer, serializer.schema, ty))
        } else {
            value.serialize(&mut Serializer::new(
                serializer.writer,
                serializer.schema,
                ty,
            ))
        };
        result?;
        Ok(())
    }

    /// Writes a positional child after checking the next expected element type.
    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        if self.expected.is_some_and(|expected| self.count >= expected) {
            return Err(AsonbError::Message("Too many elements.".into()));
        }
        let ty = match &self.kind {
            Kind::Sequence(encoding, ty) => {
                if *encoding == Encoding::MultiChunk {
                    // A one-element chunk lets an unknown-length sequence make
                    // progress without buffering or guessing its final count.
                    write_u32(self.serializer.writer, 1)?;
                }
                ty.clone()
            }
            Kind::Tuple(types) => types
                .get(self.count)
                .cloned()
                .ok_or_else(|| AsonbError::Message("Too many tuple elements.".into()))?,
            Kind::DateTime if self.count == 0 => Type::I64,
            Kind::DateTime => return Err(AsonbError::Message("Too many DateTime fields.".into())),
            _ => return Err(AsonbError::Message("Expected a sequence.".into())),
        };
        self.child(ty, value)?;
        self.count += 1;
        Ok(())
    }

    /// Requires the next Serde field name to match the next schema field exactly.
    fn field<T: Serialize + ?Sized>(&mut self, name: &str, value: &T) -> Result<(), AsonbError> {
        let Kind::Object(fields) = &self.kind else {
            return Err(AsonbError::Message("Expected Object fields.".into()));
        };
        let field = fields
            .get(self.count)
            .ok_or_else(|| AsonbError::Message("Too many Object fields.".into()))?;
        if field.name != name {
            return Err(AsonbError::Message(format!(
                "Expected field {}, found {name}.",
                field.name
            )));
        }
        self.child(field.ty.clone(), value)?;
        self.count += 1;
        Ok(())
    }

    /// Starts a pair, rejecting consecutive keys and counts above Serde's promise.
    fn key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        if self.pending {
            return Err(AsonbError::Message("Map value is missing.".into()));
        }
        if self.expected.is_some_and(|expected| self.count >= expected) {
            return Err(AsonbError::Message("Too many Map pairs.".into()));
        }
        let Kind::Map(_, key, _) = &self.kind else {
            return Err(AsonbError::Message("Expected Map.".into()));
        };
        self.child(key.clone(), value)?;
        self.pending = true;
        Ok(())
    }

    /// Completes the pending pair and only then increments the pair count.
    fn map_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        if !self.pending {
            return Err(AsonbError::Message("Map key is missing.".into()));
        }
        let Kind::Map(_, _, ty) = &self.kind else {
            return Err(AsonbError::Message("Expected Map.".into()));
        };
        self.child(ty.clone(), value)?;
        self.pending = false;
        self.count += 1;
        Ok(())
    }

    /// Checks completeness, releases buffered bytes, or writes a chunk terminator.
    fn finish(self) -> Result<(), AsonbError> {
        if self.pending || self.expected.is_some_and(|expected| expected != self.count) {
            return Err(AsonbError::Message("Incomplete container.".into()));
        }
        if let Some(buffer) = self.buffer {
            let encoding = match self.kind {
                Kind::Map(long, _, _) => {
                    if long {
                        Encoding::Long
                    } else {
                        Encoding::Regular
                    }
                }
                Kind::Sequence(encoding, _) => encoding,
                _ => unreachable!(),
            };
            write_length(self.serializer.writer, &encoding, self.count as u64)?;
            write_all(self.serializer.writer, &buffer.bytes)?;
        } else if matches!(self.kind, Kind::Sequence(Encoding::MultiChunk, _)) {
            write_u32(self.serializer.writer, 0)?;
        }
        Ok(())
    }
}

/// Routes Serde scalar callbacks through the shared exact-type value encoder.
macro_rules! serialize_numbers {($($method:ident,$rust:ty,$variant:ident);* $(;)?)=>{$(
    /// Writes this scalar only if its wire type matches the expected schema.
    fn $method(self,value:$rust)->Result<(),AsonbError>{self.scalar(Value::$variant(value))}
)*};}

impl<'a, 'w, W: Write> ser::Serializer for &'a mut Serializer<'w, W> {
    type Ok = ();
    type Error = AsonbError;
    type SerializeSeq = Compound<'a, 'w, W>;
    type SerializeTuple = Compound<'a, 'w, W>;
    type SerializeTupleStruct = Compound<'a, 'w, W>;
    type SerializeTupleVariant = Compound<'a, 'w, W>;
    type SerializeMap = Compound<'a, 'w, W>;
    type SerializeStruct = Compound<'a, 'w, W>;
    type SerializeStructVariant = Compound<'a, 'w, W>;
    serialize_numbers!(serialize_i8,i8,I8;serialize_i16,i16,I16;serialize_i32,i32,I32;serialize_i64,i64,I64;serialize_u8,u8,U8;serialize_u16,u16,U16;serialize_u64,u64,U64;serialize_f32,f32,F32;serialize_f64,f64,F64;serialize_bool,bool,Bool;serialize_char,char,Char);
    /// Writes an exact-width unsigned value, without widening or narrowing.
    fn serialize_u32(self, value: u32) -> Result<(), AsonbError> {
        self.scalar(Value::U32(value))
    }
    /// Uses the schema's String framing rather than Serde's sequence mapping.
    fn serialize_str(self, value: &str) -> Result<(), AsonbError> {
        self.scalar(Value::String(value.into()))
    }
    /// Writes Byte Data, which is intentionally distinct from List<u8>.
    fn serialize_bytes(self, value: &[u8]) -> Result<(), AsonbError> {
        self.scalar(Value::Bytes(value.into()))
    }
    /// Selects the unit None variant by name.
    fn serialize_none(self) -> Result<(), AsonbError> {
        self.variant("None", 0)?;
        Ok(())
    }
    /// Selects Some and encodes its single payload with the variant's type.
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), AsonbError> {
        let ty = self.variant("Some", 1)?;
        value.serialize(&mut Serializer::new(self.writer, self.schema, ty))
    }
    /// Rejects standalone unit, for which ASONB has no valid root type.
    fn serialize_unit(self) -> Result<(), AsonbError> {
        Err(AsonbError::Message("Unit is unsupported.".into()))
    }
    /// Rejects Rust unit structs rather than treating them as empty Objects.
    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), AsonbError> {
        Err(AsonbError::Message("Unit structs are unsupported.".into()))
    }
    /// Emits only a unit variant's schema index; there is no payload to write.
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<(), AsonbError> {
        self.variant(variant, 0)?;
        Ok(())
    }
    /// Allows only the private DateTime marker, not arbitrary newtype structs.
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<(), AsonbError> {
        if name == crate::date_time::MARKER {
            if self.ty()? != Type::DateTime {
                return Err(AsonbError::Message("Expected DateTime schema.".into()));
            }
            value.serialize(self)
        } else {
            Err(AsonbError::Message(
                "Newtype structs are unsupported.".into(),
            ))
        }
    }
    /// Checks a newtype variant's wire name and serializes its single value.
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), AsonbError> {
        let ty = self.variant(variant, 1)?;
        value.serialize(&mut Serializer::new(self.writer, self.schema, ty))
    }
    /// Requires a List and preserves whether Serde knows its element count.
    fn serialize_seq(self, length: Option<usize>) -> Result<Self::SerializeSeq, AsonbError> {
        let ty = self.ty()?;
        if !matches!(ty, Type::List(..)) {
            return Err(AsonbError::Message("Expected List schema.".into()));
        }
        Compound::new(self, ty, length)
    }
    /// Checks tuple arity, also handling the DateTime adapter's private tuple.
    fn serialize_tuple(self, length: usize) -> Result<Self::SerializeTuple, AsonbError> {
        let ty = self.ty()?;
        if !matches!(ty, Type::Tuple(_) | Type::DateTime) {
            return Err(AsonbError::Message("Expected Tuple schema.".into()));
        }
        Compound::new(self, ty, Some(length))
    }
    /// Rejects tuple structs, which are not part of the supported Rust mapping.
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct, AsonbError> {
        Err(AsonbError::Message("Tuple structs are unsupported.".into()))
    }
    /// Selects a tuple variant before writing its fixed positional payload.
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        length: usize,
    ) -> Result<Self::SerializeTupleVariant, AsonbError> {
        let ty = self.variant(variant, 2)?;
        Compound::new(self, ty, Some(length))
    }
    /// Requires a Map and chooses direct or buffered pair-count framing.
    fn serialize_map(self, length: Option<usize>) -> Result<Self::SerializeMap, AsonbError> {
        let ty = self.ty()?;
        if !matches!(ty, Type::Map(..)) {
            return Err(AsonbError::Message("Expected Map schema.".into()));
        }
        Compound::new(self, ty, length)
    }
    /// Requires an Object with exactly the number of fields Serde will emit.
    fn serialize_struct(
        self,
        _name: &'static str,
        length: usize,
    ) -> Result<Self::SerializeStruct, AsonbError> {
        let ty = self.ty()?;
        if !matches!(ty, Type::Object(..)) {
            return Err(AsonbError::Message("Expected Object schema.".into()));
        }
        Compound::new(self, ty, Some(length))
    }
    /// Selects a struct variant and checks its named payload's field count.
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        length: usize,
    ) -> Result<Self::SerializeStructVariant, AsonbError> {
        let ty = self.variant(variant, 4)?;
        Compound::new(self, ty, Some(length))
    }
    /// Requests Serde's binary representations from format-aware adapters.
    fn is_human_readable(&self) -> bool {
        false
    }
}

/// Shares positional writing between Serde's sequence and tuple traits.
macro_rules! sequence_impl {
    ($trait:ident,$method:ident) => {
        impl<W: Write> ser::$trait for Compound<'_, '_, W> {
            type Ok = ();
            type Error = AsonbError;
            /// Writes the next positional child using this container's schema.
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
                self.element(value)
            }
            /// Verifies the count and emits any deferred framing or payload.
            fn end(self) -> Result<(), AsonbError> {
                self.finish()
            }
        }
    };
}
sequence_impl!(SerializeSeq, serialize_element);
sequence_impl!(SerializeTuple, serialize_element);
sequence_impl!(SerializeTupleStruct, serialize_field);
sequence_impl!(SerializeTupleVariant, serialize_field);
/// Shares named-field writing between ordinary Objects and struct variants.
macro_rules! object_impl {
    ($trait:ident) => {
        impl<W: Write> ser::$trait for Compound<'_, '_, W> {
            type Ok = ();
            type Error = AsonbError;
            /// Validates the field's wire name before encoding its value.
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), AsonbError> {
                self.field(key, value)
            }
            /// Rejects missing fields and finalizes this container's framing.
            fn end(self) -> Result<(), AsonbError> {
                self.finish()
            }
        }
    };
}
object_impl!(SerializeStruct);
object_impl!(SerializeStructVariant);
impl<W: Write> ser::SerializeMap for Compound<'_, '_, W> {
    type Ok = ();
    type Error = AsonbError;
    /// Starts the next key/value pair, enforcing key/value alternation.
    fn serialize_key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        self.key(value)
    }
    /// Writes the value belonging to the pending key.
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), AsonbError> {
        self.map_value(value)
    }
    /// Rejects a dangling key or wrong count before completing the Map.
    fn end(self) -> Result<(), AsonbError> {
        self.finish()
    }
}

#[derive(PartialEq, Eq)]
/// Legal lifecycle states for a forward-only root List writer.
enum StreamState {
    /// No header or schema has been emitted yet.
    NotStarted,
    /// Header and schema are complete, and another chunk or terminator is legal.
    Open,
    /// The explicit zero-count terminator was written successfully.
    Finished,
    /// An operation may have emitted partial bytes; further writes are unsafe.
    Failed,
}

/// A forward-only MultiChunk List writer. Drop does not write an end marker.
///
/// Create it with [`list_stream_writer`], call `start_list` once, write any
/// number of chunks, then call `end_list`. Failed output operations permanently
/// close the writer because a partially emitted chunk cannot be undone. This
/// type never flushes the underlying destination automatically.
/// For a Result-wrapped List, `ResultStreamWriter::start_ok` opens it instead.
pub struct ListStreamWriter<'a, W: Write, T> {
    /// Borrowed sink, used directly without seeking or whole-list buffering.
    writer: &'a mut W,
    /// Validated document definitions, including the root and element schema.
    schema: Option<DocumentSchema>,
    item: Option<Type>,
    /// Guards ordering and prevents reuse after potentially partial writes.
    state: StreamState,
    /// Ties the stream's API to T without storing or owning a T value.
    marker: PhantomData<fn(&T)>,
}

impl<W: Write, T: Schema> ListStreamWriter<'_, W, T> {
    /// Writes the header and element schema, opening this root List.
    ///
    /// There are no arguments. Call exactly once before writing values. Schema
    /// errors and I/O failures leave the stream unusable; a repeated call also
    /// returns an error. This does not write any elements or flush the sink.
    ///
    /// ```
    /// let mut bytes = Vec::new();
    /// let mut stream = asonb::list_stream_writer::<u32, _>(&mut bytes);
    /// stream.start_list()?;
    /// stream.end_list()?; // An empty, explicitly terminated List.
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn start_list(&mut self) -> Result<(), AsonbError> {
        if self.state != StreamState::NotStarted {
            return Err(AsonbError::Message(
                "List stream has already started.".into(),
            ));
        }
        // Poison before touching output. Only a fully successful operation may
        // restore a usable state; unwinding or an early error leaves it closed.
        self.state = StreamState::Failed;
        let mut schema = DocumentSchema::of::<T>()?;
        let item = schema.root.clone();
        schema.root = Type::List(Encoding::MultiChunk, Box::new(schema.root));
        write_all(self.writer, &HEADER)?;
        schema.write(self.writer)?;
        self.schema = Some(schema);
        self.item = Some(item);
        self.state = StreamState::Open;
        Ok(())
    }

    /// Writes `value` as a one-element chunk without buffering the whole List.
    ///
    /// The List must be open. The borrowed value must serialize according to
    /// `T`'s schema. Serialization or I/O errors can leave a partial chunk and
    /// permanently disable further writes. The destination is not flushed.
    ///
    /// ```
    /// # let mut bytes = Vec::new();
    /// # let mut stream = asonb::list_stream_writer::<i32, _>(&mut bytes);
    /// # stream.start_list()?;
    /// stream.write(&42)?;
    /// # stream.end_list()?;
    /// # assert_eq!(asonb::de_from_slice::<Vec<i32>>(&bytes)?, [42]);
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn write(&mut self, value: &T) -> Result<(), AsonbError>
    where
        T: Serialize,
    {
        if self.state != StreamState::Open {
            return Err(AsonbError::Message("List stream is not open.".into()));
        }
        self.state = StreamState::Failed;
        let schema = self
            .schema
            .as_ref()
            .ok_or_else(|| AsonbError::Message("Missing stream schema.".into()))?;
        let item = self
            .item
            .as_ref()
            .ok_or_else(|| AsonbError::Message("Missing stream item schema.".into()))?;
        write_u32(self.writer, 1)?;
        value.serialize(&mut Serializer::new(self.writer, schema, item.clone()))?;
        self.state = StreamState::Open;
        Ok(())
    }

    /// Writes the borrowed `values` slice as one chunk, keeping element order.
    ///
    /// An empty slice writes nothing: a zero count would terminate the List.
    /// The stream must be open, and nonempty slices must fit a u32 chunk count.
    /// Count, serialization, or I/O failures leave the stream unusable; no
    /// flush is performed.
    ///
    /// ```
    /// # let mut bytes = Vec::new();
    /// # let mut stream = asonb::list_stream_writer::<i32, _>(&mut bytes);
    /// # stream.start_list()?;
    /// stream.writes(&[10, 20, 30])?;
    /// stream.writes(&[])?; // No chunk and no end marker are written.
    /// # stream.end_list()?;
    /// # assert_eq!(asonb::de_from_slice::<Vec<i32>>(&bytes)?, [10, 20, 30]);
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn writes(&mut self, values: &[T]) -> Result<(), AsonbError>
    where
        T: Serialize,
    {
        if self.state != StreamState::Open {
            return Err(AsonbError::Message("List stream is not open.".into()));
        }
        self.state = StreamState::Failed;
        let schema = self
            .schema
            .as_ref()
            .ok_or_else(|| AsonbError::Message("Missing stream schema.".into()))?;
        let item = self
            .item
            .as_ref()
            .ok_or_else(|| AsonbError::Message("Missing stream item schema.".into()))?;
        // An empty slice writes nothing, since a zero count is the end marker.
        if !values.is_empty() {
            write_u32(self.writer, values.len() as u64)?;
            for value in values {
                value.serialize(&mut Serializer::new(self.writer, schema, item.clone()))?;
            }
        }
        self.state = StreamState::Open;
        Ok(())
    }

    /// Writes the zero-count end marker and prevents any further chunks.
    ///
    /// There are no arguments. Only an open stream can be ended, and a failed
    /// marker write leaves it unusable. This does not flush the destination;
    /// flush a buffered writer yourself after releasing the stream's borrow.
    ///
    /// ```
    /// # let mut bytes = Vec::new();
    /// # let mut stream = asonb::list_stream_writer::<i32, _>(&mut bytes);
    /// # stream.start_list()?;
    /// stream.end_list()?;
    /// assert!(stream.write(&1).is_err());
    /// # Ok::<(), asonb::AsonbError>(())
    /// ```
    pub fn end_list(&mut self) -> Result<(), AsonbError> {
        if self.state != StreamState::Open {
            return Err(AsonbError::Message("List stream is not open.".into()));
        }
        self.state = StreamState::Failed;
        write_u32(self.writer, 0)?;
        self.state = StreamState::Finished;
        Ok(())
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
        Type::Object(fields) => {
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
            if let Value::Enumeration { index, payload } = value {
                let variant = variants
                    .get(*index as usize)
                    .ok_or_else(|| AsonbError::Message("Variant index out of range.".into()))?;
                let (types, values): (Vec<&Type>, Vec<&Value>) = match (variant, payload) {
                    (Variant::Unit(_), VariantValue::Unit) => (vec![], vec![]),
                    (Variant::Newtype(_, ty), VariantValue::Newtype(value)) => {
                        (vec![ty], vec![value.as_ref()])
                    }
                    (Variant::Tuple(_, types), VariantValue::Tuple(values)) => {
                        (types.iter().collect(), values.iter().collect())
                    }
                    (Variant::Struct(_, fields), VariantValue::Struct(values)) => (
                        fields.iter().map(|field| &field.ty).collect(),
                        values.iter().collect(),
                    ),
                    _ => {
                        return Err(AsonbError::Message(format!(
                            "Payload does not match variant \"{}\".",
                            variant.name()
                        )));
                    }
                };
                if types.len() != values.len() {
                    return Err(AsonbError::Message(format!(
                        "Payload arity mismatch for variant \"{}\".",
                        variant.name()
                    )));
                }
                write_u32(writer, *index as u64)?;
                for (ty, value) in types.into_iter().zip(values) {
                    write_value(writer, schema, ty, value)?;
                }
                return Ok(());
            }
        }
        Type::Reference(_) => unreachable!(),
    }
    Err(AsonbError::Message(format!("Value does not match {ty:?}.")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_stream_writer() {
        for values in [vec![], vec![11i32, 13]] {
            let mut bytes = Vec::new();
            let mut stream = result_stream_writer::<i32, String, _>(&mut bytes)
                .start_ok()
                .unwrap();
            assert!(stream.start_list().is_err());
            stream.writes(&[]).unwrap();
            for value in &values {
                stream.write(value).unwrap();
            }
            stream.end_list().unwrap();
            assert!(stream.write(&17).is_err());
            assert!(stream.end_list().is_err());
            assert_eq!(
                crate::de_from_slice::<Result<Vec<i32>, String>>(&bytes).unwrap(),
                Ok(values.clone())
            );
            let crate::ResultStreamReader::Ok(reader) =
                crate::result_stream_reader::<i32, String, _>(bytes.as_slice()).unwrap()
            else {
                panic!("expected a streamed List")
            };
            assert_eq!(reader.collect::<Result<Vec<_>, _>>().unwrap(), values);

            let schema = DocumentSchema::of::<Result<StreamListSchema<i32>, String>>().unwrap();
            let mut expected = HEADER.to_vec();
            schema.write(&mut expected).unwrap();
            Ok::<_, String>(&values)
                .serialize(&mut Serializer::new(
                    &mut expected,
                    &schema,
                    schema.root.clone(),
                ))
                .unwrap();
            assert_eq!(bytes, expected);
        }

        let values = [Some(vec![11i32, 13]), None, Some(vec![])];
        let mut bytes = Vec::new();
        let mut stream = result_stream_writer::<Option<Vec<i32>>, Vec<String>, _>(&mut bytes)
            .start_ok()
            .unwrap();
        stream.write(&values[0]).unwrap();
        stream.writes(&values[1..]).unwrap();
        stream.end_list().unwrap();
        assert_eq!(
            crate::de_from_slice::<Result<Vec<Option<Vec<i32>>>, Vec<String>>>(&bytes).unwrap(),
            Ok(values.to_vec())
        );

        let error = vec!["failed".to_owned(), "details".to_owned()];
        let mut bytes = Vec::new();
        result_stream_writer::<Option<Vec<i32>>, Vec<String>, _>(&mut bytes)
            .write_err(&error)
            .unwrap();
        assert_eq!(
            crate::de_from_slice::<Result<Vec<Option<Vec<i32>>>, Vec<String>>>(&bytes).unwrap(),
            Err(error.clone())
        );
        assert!(
            matches!(crate::result_stream_reader::<Option<Vec<i32>>, Vec<String>, _>(bytes.as_slice()).unwrap(), crate::ResultStreamReader::Err(actual) if actual == error)
        );
    }

    #[test]
    fn test_result_stream_writer_schema_requirements() {
        struct SchemaOnly;
        impl Schema for SchemaOnly {
            fn schema(_: &mut crate::SchemaBuilder) -> Result<Type, AsonbError> {
                Ok(Type::I32)
            }
        }
        struct Invalid;
        impl Schema for Invalid {
            fn schema(_: &mut crate::SchemaBuilder) -> Result<Type, AsonbError> {
                Ok(Type::Tuple(vec![]))
            }
        }
        let mut bytes = Vec::new();
        result_stream_writer::<SchemaOnly, String, _>(&mut bytes)
            .write_err(&"failed".to_owned())
            .unwrap();
        assert_eq!(
            crate::de_from_slice::<Result<Vec<i32>, String>>(&bytes).unwrap(),
            Err("failed".to_owned())
        );
        let mut bytes = Vec::new();
        result_stream_writer::<i32, SchemaOnly, _>(&mut bytes)
            .start_ok()
            .unwrap()
            .end_list()
            .unwrap();
        assert_eq!(
            crate::de_from_slice::<Result<Vec<i32>, i32>>(&bytes).unwrap(),
            Ok(vec![])
        );
        let mut bytes = Vec::new();
        assert!(
            result_stream_writer::<i32, Invalid, _>(&mut bytes)
                .start_ok()
                .is_err()
        );
        assert!(bytes.is_empty());
        assert!(
            result_stream_writer::<Invalid, String, _>(&mut bytes)
                .write_err(&"failed".to_owned())
                .is_err()
        );
        assert!(bytes.is_empty());
    }

    #[test]
    fn test_result_stream_writer_io_errors() {
        use crate::test_utils::FaultWriter;
        let mut expected = Vec::new();
        let mut stream = result_stream_writer::<i32, String, _>(&mut expected)
            .start_ok()
            .unwrap();
        stream.writes(&[11, 13]).unwrap();
        stream.end_list().unwrap();
        for fail_at in 0..expected.len() {
            let mut writer = FaultWriter {
                fail_at,
                ..FaultWriter::default()
            };
            if let Ok(mut stream) = result_stream_writer::<i32, String, _>(&mut writer).start_ok() {
                if stream.writes(&[11, 13]).is_ok() {
                    assert!(stream.end_list().is_err());
                }
                assert!(stream.write(&17).is_err());
                assert!(stream.end_list().is_err());
            }
            assert_eq!(writer.bytes, expected[..fail_at]);
        }
        let mut expected = Vec::new();
        result_stream_writer::<i32, String, _>(&mut expected)
            .write_err(&"failed".to_owned())
            .unwrap();
        for fail_at in 0..expected.len() {
            let mut writer = FaultWriter {
                fail_at,
                ..FaultWriter::default()
            };
            assert!(
                result_stream_writer::<i32, String, _>(&mut writer)
                    .write_err(&"failed".to_owned())
                    .is_err()
            );
            assert_eq!(writer.bytes, expected[..fail_at]);
        }
        let mut writer = FaultWriter {
            zero: true,
            ..FaultWriter::default()
        };
        assert!(
            result_stream_writer::<i32, String, _>(&mut writer)
                .start_ok()
                .is_err()
        );
        assert!(writer.bytes.is_empty());
    }

    #[test]
    /// Locks down primitive framing, embedded NUL handling, and canonical NaN.
    fn test_primitive_bytes() {
        assert_eq!(
            ser_to_bytes(&0x42i32).unwrap(),
            [
                0, 65, 83, 79, 78, 66, 0, 1, // header
                0, 0, 0, 0,    // reference type count
                0x15, // root value type
                0x42, 0, 0, 0 // root value
            ]
        );
        assert_eq!(
            &ser_to_bytes(&true).unwrap()[12..],
            &[
                0x40, // root value type: Boolean
                1
            ]
        );
        assert_eq!(
            &ser_to_bytes(&"a\0b").unwrap()[12..],
            &[
                0x50, // root value type: Regular String
                3, 0, 0, 0, b'a', 0, b'b'
            ]
        );
        assert_eq!(
            &ser_to_bytes(&f32::NAN).unwrap()[13..],
            &0x7fc00000u32.to_le_bytes()
        );
    }
    #[test]
    /// Checks legal/illegal transitions, batched chunks, and empty batch behavior.
    fn test_stream_states() {
        let mut output = Vec::new();
        let mut stream = list_stream_writer::<i32, _>(&mut output);
        assert!(stream.write(&1).is_err());

        stream.start_list().unwrap();
        assert!(stream.start_list().is_err());

        stream.write(&11).unwrap();
        stream.end_list().unwrap();

        assert!(stream.write(&2).is_err());
        assert!(stream.end_list().is_err());

        assert_eq!(&output[14..], &[1, 0, 0, 0, 11, 0, 0, 0, 0, 0, 0, 0]);

        let mut output = Vec::new();
        let mut stream = list_stream_writer::<i32, _>(&mut output);
        assert!(stream.writes(&[1]).is_err());
        stream.start_list().unwrap();
        stream.writes(&[11, 22]).unwrap();
        stream.writes(&[]).unwrap();
        stream.write(&33).unwrap();
        stream.end_list().unwrap();
        assert_eq!(
            &output[14..],
            &[
                2, 0, 0, 0, 11, 0, 0, 0, 22, 0, 0, 0, // chunk of 2
                1, 0, 0, 0, 33, 0, 0, 0, // chunk of 1
                0, 0, 0, 0 // end marker
            ]
        );
        assert_eq!(
            crate::de_from_slice::<Vec<i32>>(&output).unwrap(),
            [11, 22, 33]
        );
    }

    /// Generates byte-exact minimum/zero/maximum tests for each integer width.
    macro_rules! integer_test {
        ($name:ident,$rust:ty) => {
            #[test]
            /// Checks this integer width's little-endian bytes and round trip.
            fn $name() {
                for value in [<$rust>::MIN, 0, <$rust>::MAX] {
                    let bytes = ser_to_bytes(&value).unwrap();
                    assert_eq!(&bytes[13..], &value.to_le_bytes());
                    assert_eq!(crate::de_from_slice::<$rust>(&bytes).unwrap(), value);
                }
            }
        };
    }
    integer_test!(test_i8, i8);
    integer_test!(test_i16, i16);
    integer_test!(test_i32, i32);
    integer_test!(test_i64, i64);
    integer_test!(test_u8, u8);
    integer_test!(test_u16, u16);
    integer_test!(test_u32, u32);
    integer_test!(test_u64, u64);

    #[test]
    /// Preserves signed zero and subnormals while canonicalizing all NaN payloads.
    fn test_floating_point_bits() {
        for value in [
            0.0f32,
            -0.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert_eq!(&ser_to_bytes(&value).unwrap()[13..], &value.to_le_bytes());
        }
        for value in [
            0.0f64,
            -0.0,
            f64::MIN,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert_eq!(&ser_to_bytes(&value).unwrap()[13..], &value.to_le_bytes());
        }
        assert_eq!(
            &ser_to_bytes(&f64::from_bits(0xfff8000000000011)).unwrap()[13..],
            &0x7ff8000000000000u64.to_le_bytes()
        );
    }

    #[test]
    /// Checks scalar-code-point encoding, including non-ASCII boundary values.
    fn test_character_bytes() {
        for value in ['\0', 'B', '文', '\u{d7ff}', '\u{e000}', '\u{10ffff}'] {
            assert_eq!(
                &ser_to_bytes(&value).unwrap()[13..],
                &(value as u32).to_le_bytes()
            );
        }
    }

    /// A producer that deliberately does not announce its sequence length.
    struct UnknownLength;
    impl Schema for UnknownLength {
        /// The schema still knows the element type before any value is visited.
        fn schema(builder: &mut crate::SchemaBuilder) -> Result<Type, AsonbError> {
            Vec::<String>::schema(builder)
        }
    }
    impl Serialize for UnknownLength {
        /// Forces the regular List encoder to buffer until the final count is known.
        fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use ser::SerializeSeq;
            let mut seq = serializer.serialize_seq(None)?;
            seq.serialize_element("foo")?;
            seq.serialize_element("bar")?;
            seq.end()
        }
    }

    #[test]
    /// Checks that unknown-length buffering produces the same complete document.
    fn test_unknown_length_buffer() {
        let bytes = ser_to_bytes(&UnknownLength).unwrap();
        assert_eq!(
            crate::de_from_slice::<Vec<String>>(&bytes).unwrap(),
            ["foo", "bar"]
        );
        let mut output = Vec::new();
        ser_to_writer(&UnknownLength, &mut output).unwrap();
        assert_eq!(output, bytes);
    }

    #[test]
    /// Exercises short writes, WriteZero, injected failure, and stream poisoning.
    fn test_partial_and_failed_writes() {
        use crate::test_utils::FaultWriter;
        let mut writer = FaultWriter::default();
        ser_to_writer(&"hello", &mut writer).unwrap();
        assert_eq!(writer.bytes, ser_to_bytes(&"hello").unwrap());
        for fail_at in 0..18 {
            let mut writer = FaultWriter {
                fail_at,
                ..FaultWriter::default()
            };
            assert!(ser_to_writer(&"hello", &mut writer).is_err());
        }
        let mut writer = FaultWriter {
            zero: true,
            ..FaultWriter::default()
        };
        assert!(ser_to_writer(&1i32, &mut writer).is_err());
        let mut writer = FaultWriter {
            fail_at: 15,
            ..FaultWriter::default()
        };
        let mut stream = list_stream_writer::<i32, _>(&mut writer);
        stream.start_list().unwrap();
        assert!(stream.write(&1).is_err());
        assert!(stream.write(&2).is_err());
        assert!(stream.end_list().is_err());
    }
}
