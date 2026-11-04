# ASONB

_ASONB_ is a binary representation of ASON optimized for efficient storage, access, and transmission.

_ASONB_ supports streaming and limited incremental updates, making it suitable for use cases ranging from simple persistence to data-processing pipelines.

_ASONB_ avoids text-based numeric parsing and repeated object field names in the value area. With a suitable byte-slice API, string and byte payloads can be accessed without copying. Performance and storage savings depend on the data, encoding form, and implementation.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [1 Features](#1-features)
- [2 Choosing Between ASON and ASONB](#2-choosing-between-ason-and-asonb)
- [3 Library and APIs](#3-library-and-apis)
  - [3.1 Deserialization and Serialization](#31-deserialization-and-serialization)
  - [3.2 Streaming Deserialization and Serialization](#32-streaming-deserialization-and-serialization)
  - [3.3 Rust Type Mappings](#33-rust-type-mappings)
- [4 Specification](#4-specification)
  - [4.1 Document](#41-document)
    - [4.1.1 Document Header](#411-document-header)
    - [4.1.2 Document Schema](#412-document-schema)
    - [4.1.3 Root Value Binary](#413-root-value-binary)
  - [4.2 Type Definitions](#42-type-definitions)
  - [4.3 Encoding](#43-encoding)
    - [4.3.1 Numeric Types](#431-numeric-types)
    - [4.3.2 Boolean](#432-boolean)
    - [4.3.3 Character](#433-character)
    - [4.3.4 DateTime](#434-datetime)
    - [4.3.5 String](#435-string)
    - [4.3.6 Byte Data](#436-byte-data)
    - [4.3.7 List](#437-list)
    - [4.3.8 Map](#438-map)
    - [4.3.9 Tuple](#439-tuple)
    - [4.3.10 Objects](#4310-objects)
    - [4.3.11 Enumerations](#4311-enumerations)
  - [4.4 Reference Types](#44-reference-types)
    - [4.4.1 Reference Type Definition](#441-reference-type-definition)
  - [4.5 Conversion to and from ASON](#45-conversion-to-and-from-ason)
- [5 File Extension and MIME Type](#5-file-extension-and-mime-type)
- [6 Comparison with Other Binary Serialization Formats](#6-comparison-with-other-binary-serialization-formats)
- [7 Links](#7-links)
- [8 License](#8-license)

<!-- /code_chunk_output -->

## 1 Features

- Streaming: ASONB supports forward-only reading and writing for producer-consumer pipelines.
- Efficient access: ASONB allows memory-mapped access after interpreting the schema and locating the value bytes, contiguous string and byte payloads can be accessed without copying.
- Large-data support: Long and multi-chunk encodings support large strings, byte arrays, and lists without requiring the entire value to fit in memory when processed incrementally.
- Appendable: A root multi-chunk String, Byte Data, or List can be extended by appending chunks.

## 2 Choosing Between ASON and ASONB

- Choose _ASON_, the text format, when people or AI agents need to read or edit the data, such as configuration files or network service API payloads.
- Choose _ASONB_, the binary format, for inter-process communication (IPC), pipes, or storage I/O when the data does not need to be read or edited directly by people or AI agents. ASONB is also the preferred choice when performance is a priority or data volumes are very large.

## 3 Library and APIs

The Rust [asonb](https://github.com/hemashushu/asonb) crate is the reference implementation.

The crate can be added to a project with:

```sh
cargo add asonb
```

The APIs use [Serde](https://github.com/serde-rs/serde) (the serialization framework for Rust), so add it as a dependency along with `asonb` and enable Serde's `derive` feature.

```toml
[dependencies]
asonb = "3"
serde = { version = "1", features = ["derive"] }
```

### 3.1 Deserialization and Serialization

The Serde-based APIs are intended for typical application data, such as configuration files, messages, and small documents.

Consider the following ASON document:

```ason
{
    name: "foo"
    version: "0.1.0"
    dependencies: [
        "random"
        "regex"
    ]
}
```

It maps directly to the following Rust struct:

```rust
use serde::{Deserialize, Serialize};
use asonb::Schema;

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
struct Package {
    name: String,
    version: String,
    dependencies: Vec<String>,
}
```

The `asonb::Schema` derive macro supplies type information independently of the value being serialized.

Use `asonb::ser_to_writer` to serialize a value to a writer:

```rust
let package = Package {
    name: String::from("foo"),
    version: String::from("0.1.0"),
    dependencies: vec![String::from("random"), String::from("regex")],
};

let mut bytes = Vec::new();
asonb::ser_to_writer(&package, &mut bytes).unwrap();
```

To deserialize, use `asonb::de_from_reader`:

```rust
// Decode the bytes produced by ser_to_writer above.
let decoded: Package = asonb::de_from_reader(&mut bytes.as_slice()).unwrap();
```

`asonb::ser_to_bytes` returns a `Vec<u8>` directly. `asonb::de_from_slice` reads from a byte slice and can borrow contiguous string and byte payloads.

When deserializing, only Serde's `Deserialize` trait is required, the `asonb::Schema` trait is optional, because the document supplies its own schema.

```rust
let bytes = asonb::ser_to_bytes(&"Hello").unwrap();
let text: &str = asonb::de_from_slice(&bytes).unwrap();
assert_eq!(text, "Hello");
```

### 3.2 Streaming Deserialization and Serialization

For pipelines, sockets, and incremental producers, the streaming APIs process a list one or more elements at a time without materializing the entire document.

The streaming APIs accept documents whose root value is a multi-chunk `List`.

The serializer opens a list, writes elements as they become available, and finishes with an explicit end marker:

```rust
use std::io::Write;

use serde::{Deserialize, Serialize};
use asonb::Schema;

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
struct Object {
    id: i32,
    name: String,
}

fn main() {
    let o1 = Object {
        id: 11,
        name: "foo".to_owned(),
    };

    let o2 = Object {
        id: 13,
        name: "bar".to_owned(),
    };

    let mut output = std::io::stdout().lock();
    let mut stream_writer = asonb::list_stream_writer::<Object, _>(&mut output);

    // Write the document header and schema before the elements.
    stream_writer.start_list().unwrap();

    // Serialize the first element.
    stream_writer.write(&o1).unwrap();

    // Serialize the second element.
    stream_writer.write(&o2).unwrap();

    // Serialize multiple elements at once is also supported.
    // stream_writer.writes(&[o3, o4]).unwrap();

    // Write the list end marker.
    stream_writer.end_list().unwrap();

    output.flush().unwrap();

    let mut stderr = std::io::stderr().lock();
    writeln!(&mut stderr, "Streaming serialization successful!").unwrap();
}
```

The deserializer reads the root list incrementally from a binary source, such as a pipe, socket, or standard input:

1. Open a list reader on a `Read` source.
2. Call `next()` to read each element, handling any returned error. A call may block until enough bytes are available.
3. Stop when `next()` returns `None`, indicating an end marker or EOF at a valid root chunk boundary.

For example, the following program reads the list of objects written by the serializer above:

```rust
use std::io::Write;

use serde::{Deserialize, Serialize};
use asonb::Schema;

#[derive(Debug, PartialEq, Serialize, Deserialize, Schema)]
struct Object {
    id: i32,
    name: String,
}

fn main() {
    let mut input = std::io::stdin().lock();
    let mut stream_reader = asonb::list_stream_reader::<Object, _>(&mut input).unwrap();

    // Deserialize the first element.
    let o1: Object = stream_reader.next().unwrap().unwrap();
    assert_eq!(
        o1,
        Object {
            id: 11,
            name: "foo".to_owned()
        }
    );

    // Deserialize the second element.
    let o2: Object = stream_reader.next().unwrap().unwrap();
    assert_eq!(
        o2,
        Object {
            id: 13,
            name: "bar".to_owned()
        }
    );

    // Read the remaining elements until the end of the list.
    while let Some(result) = stream_reader.next() {
        let _object: Object = result.unwrap();
        // ...
    }

    let mut stderr = std::io::stderr().lock();
    writeln!(&mut stderr, "Streaming deserialization successful!").unwrap();
}
```

The example programs can be built and run with:

```sh
cargo build --examples
cd target/debug/examples
./stream-writer | ./stream-reader
```

The stream reader returns an error once and then stops. The writer rejects writes before `start_list()` or after `end_list()` or a failed write. Dropping the writer does not add an end marker. Unlike the single-document reader, the stream reader does not probe beyond an explicit end marker, so it can be used on a live socket.

### 3.3 Rust Type Mappings

Primitive types retain their widths. `usize` and `isize` use `u64` and `i64`, respectively, following Serde's mapping. `Vec<T>` and slices use Lists; Rust arrays and tuples use Tuples. Named structs use Objects, and enums use the four Enumeration payload forms. `Option<T>` and `Result<T, E>` retain their variant names. Ownership wrappers such as `Box<T>` serialize their contents, not pointer identities.

The following Rust types are not supported:

- Unit / empty tuples (`()`).
- Unit structs, such as `struct Foo;`.
- Newtype structs, such as `struct Width(u32);`.
- Tuple structs, such as `struct RGB(u8, u8, u8);`.

Empty named structs (including structs with no serialized fields) are also unsupported because Objects must not be empty. Zero-length arrays cannot use the default Tuple mapping; use a List or Byte Data adapter instead. Enum unit variants and single-value variants remain supported. Explicit Serde representations such as `#[serde(transparent)]` on a named-field struct use the represented field's type rather than an Object. The library's DateTime wrapper is supported through its dedicated adapter.

`Vec<u8>` is a List of `u8`, not Byte Data. Use `serde_bytes::Bytes`, `serde_bytes::ByteBuf`, or `#[serde(with = "serde_bytes")]` to select Byte Data.

Rust fixed-size arrays `[T; N]` use Tuples by default, with one element type per position. For a field that should use a List instead, apply `#[serde(with = "asonb::array")]`. For `[u8; N]`, apply `#[serde(with = "serde_bytes")]` to select Byte Data.

| Rust field type                 | Default mapping      | `#[serde(with = "serde_bytes")]` | `#[serde(with = "asonb::array")]` |
|---------------------------------|----------------------|----------------------------------|------------------------------------|
| `[u8; N]`                       | Tuple of `u8` values | Byte Data     | List of `u8` values |
| `[T; N]`, where `T` is not `u8` | Tuple of `T` values  | Not supported | List of `T` values  |

For chrono values, use `asonb::date_time::DateTime` at the root or the field adapter `asonb::date_time` below. The adapter converts values to UTC Unix timestamps; the original offset is not preserved, and decoded values use `+00:00`. Serialization rejects nonzero nanoseconds and offsets that are not whole-minute values in the range `-23:59` to `+23:59`, as required by ASON. Ordinary chrono serialization uses strings and must not be confused with this binary mapping.

```rust
use asonb::Schema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Schema)]
struct Record {
    #[serde(with = "asonb::date_time")]
    time: chrono::DateTime<chrono::FixedOffset>,
    #[serde(with = "serde_bytes")]
    data: Vec<u8>,
}
```

Add `chrono` and `serde_bytes` crates to the application's dependencies when using these field types and adapters. Valid wire DateTime timestamps outside chrono's range remain accessible through the low-level `Value::DateTime(i64)` representation.

## 4 Specification

### 4.1 Document

An ASONB document consists of an 8-byte document header, followed by the document schema and the root value binary.

Its layout is:

```text
[
    document header (8 bytes),
    document schema,
    root value binary
]
```

#### 4.1.1 Document Header

The document header is an 8-byte metadata block with the following content:

```text
[
    0x00,                           // Binary format indicator
    0x41, 0x53, 0x4F, 0x4E, 0x42,   // Magic number "ASONB"
    0x00,                           // 1-byte feature flags
    0x01,                           // 1-byte version number
]
```

The initial `0x00` byte distinguishes ASONB from text-based ASON when a source may contain either format. Readers must still validate the complete header.

The feature flags byte is reserved and must currently be `0x00`. Future versions may define optional features, such as alignment or padding strategies.

The format version is currently `0x01`. Readers must reject unsupported versions or feature flags rather than assume a compatible layout.

#### 4.1.2 Document Schema

The schema defines the root value's structure. It contains a table of reusable type definitions followed by the root value type:

```text
[
    reference-type definitions count (4 bytes `u32`),
    reference-type definitions (optional),
    root value type,
]
```

Reference-type definitions avoid repeating schemas, such as those for Objects, Tuples, and Enumerations. See [Reference Types](#44-reference-types) for details. The count specifies the number of definitions, not their total byte length.

The root value type, together with any definitions it references, describes the document's single root value.

Note that names in the schema use regular String value encoding: a `u32` byte length followed by UTF-8 bytes, with no String type-class byte or null terminator.

#### 4.1.3 Root Value Binary

The root value bytes immediately follow the schema. Their layout is determined by the root value type, as described in [Encoding](#43-encoding).

### 4.2 Type Definitions

ASONB represents ASON's primitive and compound value types, including numbers, Booleans, Strings, Lists, Maps, Objects, Tuples, and Enumerations.

When generating an ASONB document from ASON data, follow the ASON standard; ASONB does not extend or redefine its data types, value ranges, or precision. Object keys, Enumeration names, and variant names must be valid ASON identifiers.

To align with ASON, the following constraints apply:

- A Tuple must contain at least one element.
- An ASONB Object schema must contain at least one field, and its field names must be unique. ASON's `{}` text form is a shorthand for omitting fields with defaults in the target type; it does not represent an empty Object. ASONB object-like variant schemas must also contain at least one field, though ASON text can omit their defaulted fields in the same way.
- A tuple-like variant must contain at least two elements. Zero-element and one-element tuple-like variants are invalid; in textual ASON, the one-element form conflicts with a single-value variant. A single-value variant may itself contain a nonempty Tuple.
- ASON Maps are ordered sequences of key-value pairs, and the ASON specification does not impose a uniqueness constraint on Map keys. ASONB preserves the pairs in sequence and leaves duplicate-key handling to applications. Duplicate Object field names are rejected by schema validation.

Lists and Maps may be empty. In ASON, an empty List or Map gets its type from the expected target type; `[]` alone does not distinguish them. Schema validation checks ASONB structural constraints even for unused reference definitions and unselected variants.

Each type definition starts with a one-byte type class, followed by any type-specific bytes:

```text
[
    1-byte type class,
    additional type-specific bytes (optional)
]
```

The primitive types listed below need only a type-class byte:

| Value Type | Type Definition |
|------------|-----------------|
| i8         | 0x13            |
| i16        | 0x14            |
| i32        | 0x15            |
| i64        | 0x16            |
| u8         | 0x23            |
| u16        | 0x24            |
| u32        | 0x25            |
| u64        | 0x26            |
| f32        | 0x35            |
| f64        | 0x36            |
| Boolean    | 0x40            |
| Character  | 0x41            |
| DateTime   | 0x42            |

For numeric types, the type-class byte consists of a four-bit category and a four-bit size indicator:

```text
Most significant bit -------    ------- Least significant bit
                     0 0 0 0    0 0 0 0
                     -------    -------
                    category    size
```

The categories are:

- `1`: signed integers
- `2`: unsigned integers
- `3`: floating-point numbers

The size indicator is the base-2 logarithm of the width in bits: `0x3` means `2^3 = 8` bits, and `0x6` means `2^6 = 64` bits. Only the combinations listed in the table are currently defined; the bit layout does not imply support for other numeric widths.

String, Byte Data, and compound type definitions are described alongside their value encodings below.

### 4.3 Encoding

All integers and IEEE 754 floating-point bit patterns are stored in little-endian byte order. Strings use UTF-8. Values are concatenated without implicit alignment or padding.

Multi-chunk String, Byte Data, and List values use a zero chunk length (`0x00 0x00 0x00 0x00`) as an end marker, not as an empty data chunk. The marker occupies exactly four bytes and has no payload. An empty multi-chunk value consists of this marker, or immediate EOF if it is the root value.

EOF may replace an end marker only for a root multi-chunk value. EOF within a length prefix, payload, or incomplete list element is a truncation error. Nested values require an explicit end marker. A live source must end or supply the marker before a reader can conclude that the root value is complete.

#### 4.3.1 Numeric Types

The type determines the encoded width. For example, `i32` occupies four bytes and `f64` occupies eight. Signed integers use two's-complement representation; floating-point values use IEEE 754 binary32 or binary64.

Writers should encode NaN ("Not a Number") using these canonical quiet-NaN bit patterns:

- f32 NaN: `0x7FC0_0000` (quiet `NaN`, zero payload)
- f64 NaN: `0x7FF8_0000_0000_0000` (quiet `NaN`, zero payload)

The following complete document contains the `i32` value `0x42`:

```text
[
    /* document header */
    0x00,                           // Binary format indicator
    0x41, 0x53, 0x4F, 0x4E, 0x42,   // Magic number "ASONB"
    0x00,                           // 1-byte feature flags
    0x01,                           // 1-byte version number

    /* reference-type definitions count */
    0x00, 0x00, 0x00, 0x00,         // No reference types are defined

    /* root value type */
    0x15,                           // i32 type class

    /* root value bytes */
    0x42, 0x00, 0x00, 0x00,         // The `i32` value `0x42` in little-endian byte order
]
```

#### 4.3.2 Boolean

Boolean values occupy one byte, encoded as `i8`: `0x00` means `false`, and any nonzero byte means `true`. Writers should use `0x01` for `true`.

#### 4.3.3 Character

Character values are encoded as Unicode scalar values stored in little-endian `u32` integers.

For example, `'B'` (U+0042) is encoded as `[0x42, 0x00, 0x00, 0x00]`, and `'文'` (U+6587) is encoded as `[0x87, 0x65, 0x00, 0x00]`.

Decoders must reject values greater than `0x10FFFF` and values in the surrogate range `0xD800`–`0xDFFF`.

#### 4.3.4 DateTime

DateTime values occupy 8 bytes and represent whole seconds since the Unix epoch in UTC.

```text
[8-byte UTC timestamp `i64`]
```

The timestamp is encoded as a little-endian signed integer. Negative values represent instants before the epoch. When converting an ASON DateTime with a time offset to ASONB, convert the local date and time to UTC first (`UTC = local time - time offset`), then store its Unix timestamp. For example, `d"2024-03-16T16:30:50+08:00"` is stored as the timestamp for `2024-03-16T08:30:50Z`.

The ASONB value does not preserve the source time offset. When converting back to ASON, represent the timestamp directly as a UTC DateTime with offset `+00:00`, provided the resulting date-time is representable by ASON's DateTime syntax.

ASON DateTime does not support fractional seconds, and neither does ASONB. The chrono adapter rejects fractional seconds rather than silently discarding them. If high precision is required, consider storing the date time using a custom Object.

#### 4.3.5 String

Strings have regular and multi-chunk encoding forms. Regular Strings also have a long variant with a `u64` length.

Their type definitions and value layouts are:

| Type                       | Type Definition | Value Binary Representation                                        |
|----------------------------|-----------------|--------------------------------------------------------------------|
| Regular String             | [0x50]          | [length in `u32`, UTF-8 encoded bytes]                             |
| Long regular String        | [0x58]          | [length in `u64`, UTF-8 encoded bytes]                             |
| Multi-chunk String         | [0x51]          | [[length in `u32`, UTF-8 encoded bytes], ..., optional end marker] |

##### 4.3.5.1 Regular String

A regular String consists of a `u32` byte length followed by that many UTF-8 bytes. There is no null terminator.

The length prefix counts UTF-8 bytes, not characters. For example, a length prefix of 5 (`[0x05, 0x00, 0x00, 0x00]`) indicates 5 content bytes. Five Unicode characters may occupy more than five UTF-8 bytes.

The content may include U+0000, as in ASON strings. Decoders must read the declared number of bytes, rather than search for a null byte, and validate the content as UTF-8.

The value layout is:

```text
[
    length prefix in `u32`,
    UTF-8 encoded bytes of the string
]
```

For example, `"Hello"` is encoded as:

```text
[
    /* value bytes */
    0x05, 0x00, 0x00, 0x00,         // Byte length as u32
    0x48, 0x65, 0x6C, 0x6C, 0x6F,   // UTF-8 encoded bytes of the string "Hello"
]
```

An empty String contains only a zero length prefix:

```text
[
    /* value bytes */
    0x00, 0x00, 0x00, 0x00,         // Zero byte length
]
```

##### 4.3.5.2 Long Regular String

Long regular Strings use a `u64` length prefix instead of `u32`, allowing lengths beyond the regular form's maximum of `2^32 - 1` bytes.

The value layout is:

```text
[
    length prefix in `u64`,
    UTF-8 encoded bytes of the string
]
```

For example, `"Hello"` occupies five UTF-8 bytes:

```text
[
    /* value bytes */

    0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,     // Byte length as u64
    0x48, 0x65, 0x6C, 0x6C, 0x6F,                       // UTF-8 encoded bytes of the string "Hello"
]
```

Prefer the regular form unless the byte length may exceed `2^32 - 1`.

##### 4.3.5.3 Multi-chunk String

Multi-chunk Strings support incremental production when the total length is not known in advance.

Each nonempty chunk uses the regular String encoding. A zero length ends the value. The shared end-marker and EOF rules are described in [Encoding](#43-encoding).

The value layout is:

```text
[
    [length in `u32`, string bytes]   // chunk 1
    [length in `u32`, string bytes]   // chunk 2
    ...,
    0x00, 0x00, 0x00, 0x00,           // End marker; optional only at the document root
]
```

Each length counts the chunk's UTF-8 bytes. Every chunk must independently contain valid UTF-8; multibyte characters must not be split across chunks.

For example, the String `"HelloWorld"` can be written in two chunks, `"Hello"` and `"World"`:

```text
[
    /* value bytes */

    /* chunk 1 "Hello" */
    0x05, 0x00, 0x00, 0x00,         // Length prefix in `u32`
    0x48, 0x65, 0x6C, 0x6C, 0x6F,   // UTF-8 encoded bytes of the string "Hello"

    /* chunk 2 "World" */
    0x05, 0x00, 0x00, 0x00,         // Length prefix in `u32`
    0x57, 0x6F, 0x72, 0x6C, 0x64,   // UTF-8 encoded bytes of the string "World"

    /* chunk end marker (optional if this is the document root value) */
    0x00, 0x00, 0x00, 0x00
]
```

#### 4.3.6 Byte Data

Byte Data stores arbitrary byte sequences. It uses the same framing as String, but requires no UTF-8 validation.

Byte Data has regular and multi-chunk forms. Regular Byte Data also has a long variant with a `u64` length:

| Type                          | Type Definition | Value Binary Representation                               |
|-------------------------------|-----------------|-----------------------------------------------------------|
| Regular Byte Data             | [0x60]          | [length in `u32`, data bytes]                             |
| Long regular Byte Data        | [0x68]          | [length in `u64`, data bytes]                             |
| Multi-chunk Byte Data         | [0x61]          | [[length in `u32`, data bytes], ..., optional end marker] |

For example, the regular Byte Data encoding of `[0x01, 0x02, 0x03, 0x04]` is:

```text
[
    /* value bytes */

    0x04, 0x00, 0x00, 0x00,         // Length prefix in `u32`
    0x01, 0x02, 0x03, 0x04          // Data bytes
]
```

Empty regular Byte Data consists only of a zero length prefix:

```text
[
    /* value bytes */

    0x00, 0x00, 0x00, 0x00          // Zero byte length
]
```

Multi-chunk Byte Data uses nonempty, `u32`-length-prefixed chunks and the shared end-marker rules in [Encoding](#43-encoding). For example, six bytes can be written in two chunks:

```text
[
    /* value bytes */

    /* first chunk */
    0x04, 0x00, 0x00, 0x00,   // Length prefix in `u32`
    0x01, 0x02, 0x03, 0x04,   // Data bytes

    /* second chunk */
    0x02, 0x00, 0x00, 0x00,   // Length prefix in `u32`
    0x05, 0x06,               // Data bytes

    /* chunk end marker (optional if this is the document root value) */
    0x00, 0x00, 0x00, 0x00
]
```

#### 4.3.7 List

A List is a sequence of elements with the same type. The element type is part of the List's type definition.

Like String and Byte Data, List has regular and multi-chunk forms. Regular Lists also have a long variant with a `u64` length. List lengths count elements, not bytes.

| Type                     | Type Definition      | Value Binary Representation                                   |
|--------------------------|----------------------|---------------------------------------------------------------|
| Regular List             | [0x70, element type] | [length in `u32`, element 1, element 2, ...]                  |
| Long regular List        | [0x78, element type] | [length in `u64`, element 1, element 2, ...]                  |
| Multi-chunk List         | [0x71, element type] | [[length in `u32`, element 1, ...], ..., optional end marker] |

For example, consider the regular List `[0x11, 0x13, 0x17, 0x19]` with `i32` elements.

Its type definition is:

```text
[
    /* type definition */
    0x70,                           // Regular List type class
    0x15,                           // Element type: i32
]
```

Its value encoding is:

```text
[
    /* value bytes */
    0x04, 0x00, 0x00, 0x00,         // Element count: four

    0x11, 0x00, 0x00, 0x00,         // Element `0x11` as `i32`
    0x13, 0x00, 0x00, 0x00,         // Element `0x13` as `i32`
    0x17, 0x00, 0x00, 0x00,         // Element `0x17` as `i32`
    0x19, 0x00, 0x00, 0x00          // Element `0x19` as `i32`
]
```

The same element type can also be used in a multi-chunk List. For example, the elements `[0x11, 0x13, 0x17, 0x19, 0x21, 0x23]` can be written in chunks of two and four elements.

Its type definition is:

```text
[
    /* type definition */
    0x71,                           // Multi-chunk List type class
    0x15,                           // Element type: i32
]
```

Each chunk starts with its element count. The value encoding is:

```text
[
    /* value bytes */

    /* first chunk */
    0x02, 0x00, 0x00, 0x00,         // First chunk: two elements
    0x11, 0x00, 0x00, 0x00,         // Element `0x11` as `i32`
    0x13, 0x00, 0x00, 0x00,         // Element `0x13` as `i32`

    /* second chunk */
    0x04, 0x00, 0x00, 0x00,         // Second chunk: four elements
    0x17, 0x00, 0x00, 0x00,         // Element `0x17` as `i32`
    0x19, 0x00, 0x00, 0x00,         // Element `0x19` as `i32`
    0x21, 0x00, 0x00, 0x00,         // Element `0x21` as `i32`
    0x23, 0x00, 0x00, 0x00,         // Element `0x23` as `i32`

    /* chunk end marker (optional if this is the document root value) */
    0x00, 0x00, 0x00, 0x00
]
```

The end marker is optional only for a root multi-chunk List at EOF, as described in [Encoding](#43-encoding).

#### 4.3.8 Map

A Map is a sequence of key-value pairs. Both keys and values are ASONB values; keys are often Strings.

All keys share one type, and all values share one type, as specified in the Map type definition. Map supports only regular and long regular encodings. Their lengths count pairs, not individual keys or values. Multi-chunk Maps are not defined.

| Type             | Type Definition               | Value Binary Representation                       |
|------------------|-------------------------------|---------------------------------------------------|
| Regular Map      | [0x80, key type, value type] | [pair count in `u32`, [key bytes, value bytes], ...] |
| Long regular Map | [0x88, key type, value type] | [pair count in `u64`, [key bytes, value bytes], ...] |

For example, consider this ASON Map:

```ason
[
    "red": "#ff0000",
    "green": "#00ff00"
]
```

Its type definition is:

```text
[
    /* type definition */
    0x80,                           // Regular Map type class
    0x50,                           // Key type: regular String
    0x50,                           // Value type: regular String
]
```

Its value encoding is:

```text
[
    /* value bytes */

    /* Pair count */
    0x02, 0x00, 0x00, 0x00,                             // Two key-value pairs

    /* key "red" */
    0x03, 0x00, 0x00, 0x00,                             // Key byte length as u32
    0x72, 0x65, 0x64,                                   // "red" as String

    /* value "#ff0000" */
    0x07, 0x00, 0x00, 0x00,                             // Value byte length as u32
    0x23, 0x66, 0x66, 0x30, 0x30, 0x30, 0x30,           // "#ff0000" as String

    /* key "green" */
    0x05, 0x00, 0x00, 0x00,                             // Key byte length as u32
    0x67, 0x72, 0x65, 0x65, 0x6E,                       // "green" as String

    /* value "#00ff00" */
    0x07, 0x00, 0x00, 0x00,                             // Value byte length as u32
    0x23, 0x30, 0x30, 0x66, 0x66, 0x30, 0x30,           // "#00ff00" as String
]
```

The wire format records pairs in sequence. An unordered application map may not preserve that order, so applications that need order-preserving ASON conversion must use an ordered representation. Duplicate-key handling is an application-level policy; conversion must follow the ASON standard.

#### 4.3.9 Tuple

A Tuple has a fixed number of ordered elements, each of which may have a different type.

Its type definition contains `0x90`, the element count, and the element types in order:

```text
[0x90, number of elements in `u32`, type list]
```

The value is the concatenation of the element encodings in that same order, with no additional count in the value area:

```text
[value list]
```

For example, consider this ASON Tuple:

```ason
("red", 255, true)
```

Its type definition is:

```text
[
    /* type definition */
    0x90,                           // Tuple type class

    0x03, 0x00, 0x00, 0x00,         // Element count: three

    0x50,                           // First element type String
    0x15,                           // Second element type `i32`
    0x40,                           // Third element type Boolean
]
```

Its value encoding is:

```text
[
    /* value bytes */

    /* first element "red" */
    0x03, 0x00, 0x00, 0x00,         // String byte length as u32
    0x72, 0x65, 0x64,               // "red" as String

    /* second element `255` */
    0xFF, 0x00, 0x00, 0x00,         // 255 as i32

    /* third element `true` */
    0x01                            // Boolean `true` as `i8`
]
```

#### 4.3.10 Objects

An Object has a fixed set of named fields whose types and order are defined in the schema.

The type definition for an Object starts with `0xA0`, followed by the object name as a regular string, the number of fields, and then each field's key as a regular string and its type in order:

```text
[
    0xA0,
    object name as a regular String,
    field count in `u32`,
    [field name as a regular String, field type]...
]
```

The object name is optional metadata and is not required to locate or decode fields. An unnamed Object uses an empty regular String (`[0x00, 0x00, 0x00, 0x00]`).

Field names must be unique. Field values appear in schema order.

The value area contains only the field encodings, without repeated names:

```text
[value list]
```

For example, consider an ASONB document with an Object:

```ason
{
    id: 123,
    name: "Alice",
    active: true
}
```

Its type definition is:

```text
[
    /* type definition */

    0xA0,                                       // Object type class
    0x00, 0x00, 0x00, 0x00,                     // Empty object name
    0x03, 0x00, 0x00, 0x00,                     // Field count: three

    /* field "id": i32 */
    0x02, 0x00, 0x00, 0x00,                     // Field name byte length
    0x69, 0x64,                                 // "id" as String
    0x15,                                       // Field type: i32

    /* field "name": regular String */
    0x04, 0x00, 0x00, 0x00,                     // Field name byte length
    0x6E, 0x61, 0x6D, 0x65,                     // "name" as String
    0x50,                                       // Field type: regular String

    /* field "active": Boolean */
    0x06, 0x00, 0x00, 0x00,                     // Field name byte length
    0x61, 0x63, 0x74, 0x69, 0x76, 0x65,         // "active" as String
    0x40,                                       // Field type: Boolean
]
```

Its value encoding is:

```text
[
    /* value bytes */

    0x7B, 0x00, 0x00, 0x00,                     // `123` in `i32`
    0x05, 0x00, 0x00, 0x00,                     // String byte length as u32
    0x41, 0x6C, 0x69, 0x63, 0x65,               // "Alice" as String
    0x01,                                       // Boolean `true`
]
```

> Object values use the concatenated field encodings with no implicit padding. Fixed-width fields therefore have predictable byte offsets, but these offsets need not satisfy native alignment requirements. Memory mapping does not make the bytes a native Rust or C struct: access must account for alignment, little-endian byte order, and value validation. Variable-width fields require scanning or an external index to locate subsequent fields.

#### 4.3.11 Enumerations

An Enumeration defines a set of named variants, each with its own payload shape.

Its type definition contains `0xB0`, the enumeration name, the variant count, and the variant definitions in order:

```text
[
    0xB0,
    enumeration name as a regular String,
    variant count in `u32`,
    list of variant definitions
]
```

The enumeration name is required and must not be empty. It is encoded as a regular String. Conversion to ASON also requires the name to be a valid ASON identifier.

Each variant definition includes its name and any payload types. Variant names must be unique within the Enumeration; field names within an object-like variant must also be unique. Four forms are supported:

| Variant Form                    | Variant definition                                                                                 |
|---------------------------------|----------------------------------------------------------------------------------------------------|
| No payload                      | [0x00, variant name as a regular String] |
| Single-value payload            | [0x01, variant name as a regular String, value type] |
| Tuple-like payload              | [0x02, variant name as a regular String, element count in `u32`, type list] |
| Object-like payload             | [0x04, variant name as a regular String, field count in `u32`, [field name as a regular String, field type]...] |

An enumeration value starts with the variant's zero-based index (`u32`), followed by its payload, if any. A single-value payload uses its declared type's encoding; tuple-like and object-like payloads concatenate their element or field encodings in schema order, without additional counts or names.

```text
[variant index in `u32`, payload (if any)]
```

The index identifies a variant in this document's schema, not a stable identifier across documents or application types. Decoders must reject out-of-range indices. When mapping to an application enumeration, match variants by name rather than assuming the same index order.

For example, `Option` has two variants: `None` with no payload and `Some` with one `i32` payload. Its type definition is:

```text
[
    /* type definition */

    0xB0,                                           // Enumeration type class

    /* enumeration name "Option" as String */
    0x06, 0x00, 0x00, 0x00,
    0x4F, 0x70, 0x74, 0x69, 0x6F, 0x6E,            // "Option" as String

    /* variant count */
    0x02, 0x00, 0x00, 0x00,                         // Two variants

    /* variant 0: "None" without a payload */
    0x00,                                           // No-payload variant form
    0x04, 0x00, 0x00, 0x00,                         // Variant name byte length
    0x4E, 0x6F, 0x6E, 0x65,                         // "None" as String

    /* variant 1: "Some" with one i32 payload */
    0x01,                                           // Single-value variant form
    0x04, 0x00, 0x00, 0x00,                         // Variant name byte length
    0x53, 0x6F, 0x6D, 0x65,                         // "Some" as String

    /* payload type: i32 */
    0x15
]
```

The value encoding of `Option::None` is:

```text
[
    0x00, 0x00, 0x00, 0x00          // variant index 0 ("None") in `u32`
]
```

The value encoding of `Option::Some(0x42)` is:

```text
[
    0x01, 0x00, 0x00, 0x00,         // Variant index 1 ("Some") as u32
    0x42, 0x00, 0x00, 0x00          // value `0x42` as `i32`
]
```

### 4.4 Reference Types

Reference types allow schemas to reuse type definitions, including those for Objects, Tuples, and Enumerations. Each table entry receives a zero-based index in definition order.

A type reference is a schema abbreviation, not a pointer or shared value. Its value is encoded exactly as the referenced type, with no reference index in the value area. ASONB supports value data only: object identity, shared storage, and pointer values are not represented.

The table is optional, even when complex types are defined inline. An empty table has a count of zero. All reference indices must be less than the table count. Forward references may reuse later definitions, but reference dependencies must be acyclic; recursive or mutually recursive type definitions are not supported.

#### 4.4.1 Reference Type Definition

Each entry in the reference-type table is a complete type definition. Any supported type can be stored in the table and referenced using `[0xC0, index in u32]`.

For example, consider these two Object schemas:

```text
Object Address: {
    street: String,
    city: String,
}

Object Person: {
    name: String,
    age: i32,
    address: Address
}
```

Their reference-type table is:

```text
[
    /* Reference type definition count */
    0x02, 0x00, 0x00, 0x00,                     // 2 reference types defined

    /* Reference type definitions */

    /* Object type: Address */
    0xA0,                                       // Object type class
    0x07, 0x00, 0x00, 0x00,                     // Object name byte length
    0x41, 0x64, 0x64, 0x72, 0x65, 0x73, 0x73,   // "Address" as String
    0x02, 0x00, 0x00, 0x00,                     // Field count: two

    0x06, 0x00, 0x00, 0x00,                     // Field name byte length
    0x73, 0x74, 0x72, 0x65, 0x65, 0x74,         // "street" as String
    0x50,                                       // Field type: regular String

    0x04, 0x00, 0x00, 0x00,                     // Field name byte length
    0x63, 0x69, 0x74, 0x79,                     // "city" as String
    0x50,                                       // Field type: regular String

    /* Object type: Person */
    0xA0,                                       // Object type class
    0x06, 0x00, 0x00, 0x00,                     // Object name byte length
    0x50, 0x65, 0x72, 0x73, 0x6F, 0x6E,         // "Person" as String
    0x03, 0x00, 0x00, 0x00,                     // Field count: three

    0x04, 0x00, 0x00, 0x00,                     // Field name byte length
    0x6E, 0x61, 0x6D, 0x65,                     // "name" as String
    0x50,                                       // Field type: regular String

    0x03, 0x00, 0x00, 0x00,                     // Field name byte length
    0x61, 0x67, 0x65,                           // "age" as String
    0x15,                                       // Field type: i32

    0x07, 0x00, 0x00, 0x00,                     // Field name byte length
    0x61, 0x64, 0x64, 0x72, 0x65, 0x73, 0x73,   // "address" as String
    0xC0, 0x00, 0x00, 0x00, 0x00,               // Type reference: index zero (Address)
]
```

The `address` field uses `[0xC0, 0x00, 0x00, 0x00, 0x00]` to reuse the schema at index zero, `Address`. Each Person value contains its own encoded Address value, not a pointer to another value.

Note that this example shows only the reference-type table; a complete document also needs a header, a root value type, and the root value bytes.

### 4.5 Conversion to and from ASON

The [ASON standard](https://github.com/hemashushu/ason/blob/main/docs/specification.md) governs conversion between ASON text and ASONB. The binary format does not extend ASON's text syntax or relax its value constraints. This crate does not implement ASON text conversion.

Conversion preserves values when ASON can represent them, but not source formatting, comments, or the choice of binary encoding form. Converting from ASON requires a schema supplied by the application or derived from the expected target type where sufficient type information is available. In particular, ASON uses that context to determine whether `[]` is an empty List or Map and to resolve its element, key, and value types. When an ASON `[]` cannot be inferred from context, ASONB uses `List<i32>` as the fallback type.

DateTime conversion applies an ASON time offset when producing the UTC timestamp. Converting back produces a UTC DateTime with offset `+00:00` when the timestamp is representable by ASON's DateTime syntax; the original offset is not recoverable. The chrono adapter rejects nonzero fractional seconds because ASON DateTime and the ASONB representation both use whole-second precision.

## 5 File Extension and MIME Type

The file extension for ASONB documents is `.asonb`, and the MIME type is `application/asonb`.

## 6 Comparison with Other Binary Serialization Formats

These formats make different trade-offs; none is uniformly smaller or faster for every workload.

| Format | Relationship to ASONB |
|--------|-----------------------|
| BSON | Stores field names with each document and provides a different value type system. ASONB stores Object field names in the schema rather than repeating them for every value of that type. |
| Protocol Buffers | Uses numbered fields and schema-based messages, with established schema-evolution rules. ASONB embeds its schema in each document and represents ASON's value types directly. |
| Avro | Also separates schemas from value encodings and supports schema resolution. ASONB uses its own type system and provides multi-chunk String, Byte Data, and List encodings. |
| CBOR | Uses per-value type information, supports tags, and offers indefinite-length encodings. ASONB instead uses a document schema to describe value layouts. |
| MessagePack | Uses per-value type information and supports binary and extension types. ASONB describes repeated structures through a schema and defines multi-chunk encodings for incremental production. |

ASONB's focus is schema-described ASON values, forward-only streaming, and predictable offsets for fixed-width layouts. Variable-width layouts still require scanning or an external index. Storage size, access speed, and conversion cost should be compared using representative data and implementations rather than inferred from the format alone.

## 7 Links

- [ASON Specification](https://github.com/hemashushu/ason/blob/main/docs/specification.md)
- [ASON Library (Rust)](https://github.com/hemashushu/ason)

## 8 License

This project is licensed under the MPL 2.0 License with additional terms. See [LICENSE](./LICENSE) and [LICENSE.additional](./LICENSE.additional).
