# ASONB

_ASONB_ is a binary representation of ASON optimized for efficient storage, access, and transmission.

ASONB supports streaming and limited incremental updates, making it suitable for use cases ranging from simple persistence to data-processing pipelines.

ASONB avoids text-based numeric parsing and repeated object field names in the value area. With a suitable byte-slice API, string and byte payloads can be accessed without copying. Performance and storage savings depend on the data, encoding form, and implementation.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [1 Features](#1-features)
- [2 Library and APIs](#2-library-and-apis)
  - [2.1 Deserialization and Serialization](#21-deserialization-and-serialization)
  - [2.2 Streaming Deserialization and Serialization](#22-streaming-deserialization-and-serialization)
  - [2.3 Rust Type Mappings](#23-rust-type-mappings)
  - [2.4 Validation and Error Handling](#24-validation-and-error-handling)
- [3 Quick Reference](#3-quick-reference)
  - [3.1 Document](#31-document)
    - [3.1.1 Document Header](#311-document-header)
    - [3.1.2 Document Schema](#312-document-schema)
    - [3.1.3 Root Value Binary](#313-root-value-binary)
  - [3.2 Type Definitions](#32-type-definitions)
  - [3.3 Encoding](#33-encoding)
    - [3.3.1 Numeric Types](#331-numeric-types)
    - [3.3.2 Boolean](#332-boolean)
    - [3.3.3 Character](#333-character)
    - [3.3.4 DateTime](#334-datetime)
    - [3.3.5 String](#335-string)
    - [3.3.6 Byte Data](#336-byte-data)
    - [3.3.7 List](#337-list)
    - [3.3.8 Map](#338-map)
    - [3.3.9 Tuple](#339-tuple)
    - [3.3.10 Objects](#3310-objects)
    - [3.3.11 Enumerations](#3311-enumerations)
  - [3.4 Reference Types](#34-reference-types)
    - [3.4.1 Reference Type Definition](#341-reference-type-definition)
  - [3.5 Conversion to and from ASON](#35-conversion-to-and-from-ason)
- [4 File Extension and MIME Type](#4-file-extension-and-mime-type)
- [5 Comparison with Other Binary Serialization Formats](#5-comparison-with-other-binary-serialization-formats)
- [6 Links](#6-links)
- [7 License](#7-license)

<!-- /code_chunk_output -->

## 1 Features

- Streaming: ASONB supports forward-only reading and writing for producer-consumer pipelines.
- Efficient access: ASONB allows memory-mapped access after interpreting the schema and locating the value bytes, contiguous string and byte payloads can be accessed without copying.
- Large-data support: Long and multi-chunk encodings support large strings, byte arrays, and lists without requiring the entire value to fit in memory when processed incrementally.
- Appendable: A root multi-chunk String, Byte Data, or List can be extended by appending chunks.

## 2 Library and APIs

The Rust [asonb](https://github.com/hemashushu/asonb) crate is the reference implementation.

This repository is a Cargo workspace: `crates/asonb` contains the main library, and `crates/asonb-derive` contains the procedural macro. Run `cargo test --workspace` from the repository root to test both crates.

The crate can be added to a project with:

```sh
cargo add asonb
```

The APIs use [Serde](https://github.com/serde-rs/serde) (the serialization framework for Rust), so add it as a dependency along with `asonb` and enable Serde's `derive` feature.

```toml
[dependencies]
asonb = "3.1"
serde = { version = "1.0", features = ["derive"] }
```

### 2.1 Deserialization and Serialization

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

### 2.2 Streaming Deserialization and Serialization

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

### 2.3 Rust Type Mappings

Primitive types retain their widths. `usize` and `isize` use `u64` and `i64`, respectively, following Serde's mapping. `Vec<T>` and slices use Lists; Rust arrays and tuples use Tuples. Named structs use Objects, and enums use the four Enumeration payload forms. `Option<T>` and `Result<T, E>` retain their variant names. Ownership wrappers such as `Box<T>` serialize their contents, not pointer identities.

Binary-only mappings also support `()` as an empty Tuple, unit structs as empty Objects, tuple structs as Tuples, and newtype or transparent structs as their payload. These mappings do not imply that the values can be converted to ASON text.

`Vec<u8>` is a List of `u8`, not Byte Data. Use `serde_bytes::Bytes`, `serde_bytes::ByteBuf`, or `#[serde(with = "serde_bytes")]` to select Byte Data.

Rust fixed-size arrays `[T; N]` use Tuples by default, with one element type per position. For a field that should use a List instead, apply `#[serde(with = "asonb::array")]`. This adapter supports any `N`, including zero and lengths greater than 32, and deserialization requires exactly `N` elements. For `[u8; N]`, `#[serde(with = "serde_bytes")]` selects Byte Data; this is separate from both the default Tuple mapping and the `asonb::array` List mapping. `Vec<u8>` remains a List of `u8` unless annotated with `serde_bytes`.

| Rust field type                 | Default mapping      | `#[serde(with = "serde_bytes")]` | `#[serde(with = "asonb::array")]` |
|---------------------------------|----------------------|----------------------------------|------------------------------------|
| `[u8; N]`                       | Tuple of `u8` values | Byte Data     | List of `u8` values |
| `[T; N]`, where `T` is not `u8` | Tuple of `T` values  | Not supported | List of `T` values  |

The default Tuple mapping follows Serde's array implementations. ASONB's binary format supports empty Tuples; this binary-only mapping does not imply that the value can be converted to ASON text.

For chrono values, use `asonb::date_time::DateTime` at the root or the field adapter below. It preserves timestamp, nanoseconds, and numeric offset directly, including offsets with second precision. Ordinary chrono serialization uses strings and must not be confused with this binary mapping.

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

Add `chrono` and `serde_bytes` crates to the application's dependencies when using these field types and adapters. Valid wire DateTime values outside chrono's range remain accessible through the low-level `Value::DateTime` representation. Leap-second nanoseconds are rejected because ASONB requires nanoseconds below one billion.

`Schema` supports generics, lifetimes, const generics, Serde renaming, skipped fields, and the documented adapters. Value-dependent `skip_serializing_if`, `flatten`, and internally tagged, adjacently tagged, or untagged enums are rejected because they do not match this static schema mapping. Other custom serializers require an explicit `#[asonb(schema_with = "path")]` schema function accepting `&mut SchemaBuilder` and returning `Result<Type, AsonbError>`. Runtime serialization still checks the emitted shape against that schema. `deserialize_any` is not supported; use a concrete target type or the low-level value APIs.

For explicit encoding choices, `DocumentSchema`, `Type`, and `Value` expose all long, fixed-capacity, and multi-chunk forms through `ser_value_to_writer`, `de_value_from_reader`, and `de_value_from_slice`. Fixed String slots remain raw bytes; no NUL trimming, padding validation, or logical-length inference is performed. Multi-chunk payloads are joined for owned String/Byte Data targets and cannot be returned as a contiguous borrowed `&str` or byte slice.

### 2.4 Validation and Error Handling

Schemas and values have no library-imposed resource quotas. Serialization rejects recursive Rust type schemas before emitting document bytes and checks explicit document schemas for cyclic references. Deserialization checks every document schema definition, including unused definitions, for cyclic references before reading values. Shared, acyclic references are allowed; graph/pointer identity is not supported.

There are no size, element-count, nesting-depth, or buffering quotas for schema names and definitions, String and Byte Data payloads, List and Map contents, or cumulative list stream length. The library still checks length-prefix limits imposed by the format, fixed-capacity framing, arithmetic overflow, platform size conversion, and input validity, and uses fallible allocation for owned payloads and unknown-length value buffers. Deeply nested documents can exhaust the call stack; large schemas and values can exhaust memory. Use streaming APIs when processing large data incrementally. Applications must provide their own resource policy when reading untrusted documents, including allocations made by custom Serde visitors.

Single-document readers reject trailing bytes. Confirming EOF may block on a live source, so use them for finite documents and use the list streaming APIs for pipelines. Invalid headers, unsupported flags or versions, malformed schemas, invalid UTF-8, invalid characters, out-of-range variants, and truncated input produce `AsonbError`. Input and I/O errors include a byte offset where available.

Serialization errors may leave a partial document in the writer. The library does not promise transactional writes or rollback; applications requiring atomic persistence should write to a temporary destination and commit it only after serialization succeeds.

## 3 Quick Reference

### 3.1 Document

An ASONB document consists of an 8-byte document header, followed by the document schema and the root value binary.

Its layout is:

```text
[
    document header (8 bytes),
    document schema,
    root value binary
]
```

#### 3.1.1 Document Header

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

#### 3.1.2 Document Schema

The schema defines the root value's structure. It contains a table of reusable type definitions followed by the root value type:

```text
[
    reference-type definitions count (4 bytes `u32`),
    reference-type definitions (optional),
    root value type,
]
```

Reference-type definitions avoid repeating schemas, such as those for Objects, Tuples, and Enumerations. See [Reference Types](#34-reference-types) for details. The count specifies the number of definitions, not their total byte length.

The root value type, together with any definitions it references, describes the document's single root value.

Note that names in the schema use regular String value encoding: a `u32` byte length followed by UTF-8 bytes, with no String type-class byte or null terminator.

#### 3.1.3 Root Value Binary

The root value bytes immediately follow the schema. Their layout is determined by the root value type, as described in [Encoding](#33-encoding).

### 3.2 Type Definitions

ASONB represents ASON's primitive and compound value types, including numbers, Booleans, Strings, Lists, Maps, Objects, Tuples, and Enumerations.

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

### 3.3 Encoding

All integers and IEEE 754 floating-point bit patterns are stored in little-endian byte order. Strings use UTF-8. Values are concatenated without implicit alignment or padding.

Fixed-capacity String, Byte Data, and List encodings are intended for low-level systems that need reserved storage or predictable layouts. ASONB specifies their capacities and framing, but does not prescribe logical content lengths, padding values, or how applications distinguish content from unused capacity. The zero-filled examples below illustrate an application convention, not a format requirement. Fixed-capacity Lists still contain the number of element encodings specified by their capacity, allowing readers to locate subsequent values.

Multi-chunk String, Byte Data, and List values use a zero chunk length (`0x00 0x00 0x00 0x00`) as an end marker, not as an empty data chunk. The marker occupies exactly four bytes and has no payload. An empty multi-chunk value consists of this marker, or immediate EOF if it is the root value.

EOF may replace an end marker only for a root multi-chunk value. EOF within a length prefix, payload, or incomplete list element is a truncation error. Nested values require an explicit end marker. A live source must end or supply the marker before a reader can conclude that the root value is complete.

Lengths and counts must be checked for overflow before calculating offsets or allocating storage. Applications processing untrusted documents should enforce resource limits appropriate to their environment; the Rust library imposes no schema or value resource quotas.

#### 3.3.1 Numeric Types

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

#### 3.3.2 Boolean

Boolean values occupy one byte, encoded as `i8`: `0x00` means `false`, and any nonzero byte means `true`. Writers should use `0x01` for `true`.

#### 3.3.3 Character

Character values are encoded as Unicode scalar values stored in little-endian `u32` integers.

For example, `'B'` (U+0042) is encoded as `[0x42, 0x00, 0x00, 0x00]`, and `'文'` (U+6587) is encoded as `[0x87, 0x65, 0x00, 0x00]`.

Decoders must reject values greater than `0x10FFFF` and values in the surrogate range `0xD800`–`0xDFFF`.

#### 3.3.4 DateTime

DateTime values occupy 16 bytes: a timestamp, a fractional second, and a UTC offset.

```text
[8-byte timestamp `i64`, 4-byte nanoseconds `u32`, 4-byte timezone offset `i32`]
```

The fields are:

- Timestamp (`i64`, 8 bytes): whole seconds since the Unix epoch, `1970-01-01T00:00:00Z`. Negative values represent instants before the epoch.

- Nanoseconds (`u32`, 4 bytes): the fractional second, in `0..=999_999_999`. Decoders must reject larger values. The instant is `timestamp + nanoseconds / 1_000_000_000` seconds from the epoch, using fractional rather than integer division. The timestamp is rounded down, including before the epoch: `timestamp = -1` and `nanoseconds = 500_000_000` represent half a second before the epoch.

- UTC offset (`i32`, 4 bytes): the offset in seconds. A positive value means local time is ahead of UTC; for example, `+08:00` is `28_800`. The timestamp already represents a UTC-based instant, so this offset must not be subtracted from it again. UTC uses zero. The field preserves a numeric offset, not a named time zone such as `Asia/Shanghai`.

Conversion to ASON must respect the DateTime syntax and range supported by the ASON standard; see [Conversion to and from ASON](#35-conversion-to-and-from-ason).

#### 3.3.5 String

Strings have three encoding forms: regular, fixed-capacity, and multi-chunk. The regular and fixed-capacity forms also have long variants with `u64` lengths or capacities.

Their type definitions and value layouts are:

| Type                       | Type Definition | Value Binary Representation                                        |
|----------------------------|-----------------|--------------------------------------------------------------------|
| Regular String             | [0x50]          | [length in `u32`, UTF-8 encoded bytes]                             |
| Long regular String        | [0x58]          | [length in `u64`, UTF-8 encoded bytes]                             |
| Fixed-capacity String      | [0x51, capacity in `u32`] | [capacity bytes; application-defined content and padding] |
| Long fixed-capacity String | [0x59, capacity in `u64`] | [capacity bytes; application-defined content and padding] |
| Multi-chunk String         | [0x52]          | [[length in `u32`, UTF-8 encoded bytes], ..., optional end marker] |

##### 3.3.5.1 Regular String

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

##### 3.3.5.2 Long Regular String

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

##### 3.3.5.3 Fixed-capacity String

Fixed-capacity Strings reserve a fixed number of bytes, specified as a `u32` in the type definition. The value occupies exactly that many bytes, with no length prefix or null terminator. Applications determine the logical string length and how unused bytes are filled or interpreted.

This form follows the low-level conventions described in [Encoding](#33-encoding), rather than defining a general-purpose string representation with a recoverable content length.

The following complete document reserves eight bytes. The application stores `"Hello"` and fills the remaining bytes with zeros:

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
    0x51,                           // Fixed-capacity String type class
    0x08, 0x00, 0x00, 0x00,         // Capacity: eight bytes

    /* root value bytes */
    0x48, 0x65, 0x6C, 0x6C, 0x6F,   // UTF-8 encoded bytes of the string "Hello"
    0x00, 0x00, 0x00,               // Application-defined zero padding
]
```

> ASONB does not prescribe how to recover the content length. An application may use a separate length field or its own content restrictions. Treating the first zero byte as the end of the string is lossless only if the application forbids U+0000 in the content.

##### 3.3.5.4 Long Fixed-capacity String

Long fixed-capacity Strings use a `u64` capacity in the type definition instead of `u32`. All other layout rules are unchanged.

##### 3.3.5.5 Multi-chunk String

Multi-chunk Strings support incremental production when the total length is not known in advance.

Each nonempty chunk uses the regular String encoding. A zero length ends the value. The shared end-marker and EOF rules are described in [Encoding](#33-encoding).

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

#### 3.3.6 Byte Data

Byte Data stores arbitrary byte sequences. It uses the same framing as String, but requires no UTF-8 validation.

It has regular, fixed-capacity, and multi-chunk forms, with long variants of the first two:

| Type                          | Type Definition | Value Binary Representation                      |
|-------------------------------|-----------------|--------------------------------------------------|
| Regular Byte Data             | [0x60]          | [length in `u32`, data bytes]                    |
| Long regular Byte Data        | [0x68]          | [length in `u64`, data bytes]                    |
| Fixed-capacity Byte Data      | [0x61, capacity in `u32`] | [capacity bytes]                       |
| Long fixed-capacity Byte Data | [0x69, capacity in `u64`] | [capacity bytes]                       |
| Multi-chunk Byte Data         | [0x62]          | [[length in `u32`, data bytes], ..., optional end marker] |

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

Fixed-capacity Byte Data occupies exactly the specified number of bytes. Its logical length and unused bytes are application-defined. For example, an application may store four data bytes in an eight-byte slot and fill the remaining bytes with zeros:

```text
[
    /* value bytes */

    0x01, 0x02, 0x03, 0x04,         // Content bytes
    0x00, 0x00, 0x00, 0x00          // Application-defined zero padding
]
```

Multi-chunk Byte Data uses nonempty, `u32`-length-prefixed chunks and the shared end-marker rules in [Encoding](#33-encoding). For example, six bytes can be written in two chunks:

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

#### 3.3.7 List

A List is a sequence of elements with the same type. The element type is part of the List's type definition.

Like String and Byte Data, List has regular, fixed-capacity, and multi-chunk forms, with long variants of the first two. List lengths and capacities count elements, not bytes.

| Type                     | Type Definition      | Value Binary Representation                                   |
|--------------------------|----------------------|---------------------------------------------------------------|
| Regular List             | [0x70, element type] | [length in `u32`, element 1, element 2, ...]                  |
| Long regular List        | [0x78, element type] | [length in `u64`, element 1, element 2, ...]                  |
| Fixed-capacity List      | [0x71, capacity in `u32`, element type] | [capacity element encodings]              |
| Long fixed-capacity List | [0x79, capacity in `u64`, element type] | [capacity element encodings]              |
| Multi-chunk List         | [0x72, element type] | [[length in `u32`, element 1, ...], ..., optional end marker] |

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
    0x72,                           // Multi-chunk List type class
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

The end marker is optional only for a root multi-chunk List at EOF, as described in [Encoding](#33-encoding).

Fixed-capacity Lists store exactly the capacity's number of element encodings, without a value-area count. Applications decide which slots are logically used and what values occupy unused slots; ASONB defines no default padding value. The encoded byte size is fixed only when the element type also has a fixed encoded size.

For example, an application may fill unused `i32` slots with zeros or unused regular String slots with empty Strings. These are application conventions, not serialization requirements.

The following example uses a fixed-capacity List of fixed-capacity Strings. The application stores two logical elements:

`["foo", "beef"]`

The capacities are:

- Four elements in the List.
- Four bytes in each String slot.

Its type definition is:

```text
[
    /* type definition */
    0x71,                           // Fixed-capacity List type class
    0x04, 0x00, 0x00, 0x00,         // List capacity: four elements
    0x51,                           // Element type: fixed-capacity String
    0x04, 0x00, 0x00, 0x00,         // String capacity: four bytes
]
```

Using application-defined zero padding, its value encoding is:

```text
[
    /* value bytes */

    /* first element "foo" */
    0x66, 0x6F, 0x6F, 0x00,             // "foo" with one padding byte

    /* second element "beef" */
    0x62, 0x65, 0x65, 0x66,             // "beef"

    /* Unused third slot */
    0x00, 0x00, 0x00, 0x00,             // Application-defined zero-filled slot

    /* Unused fourth slot */
    0x00, 0x00, 0x00, 0x00,             // Application-defined zero-filled slot
]
```

Use regular or multi-chunk Lists for general-purpose serialization; the fixed-capacity form requires application-specific conventions.

#### 3.3.8 Map

A Map is a sequence of key-value pairs. Both keys and values are ASONB values; keys are often Strings.

All keys share one type, and all values share one type, as specified in the Map type definition. Map supports only regular and long regular encodings. Their lengths count pairs, not individual keys or values. Fixed-capacity and multi-chunk Maps are not defined.

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

#### 3.3.9 Tuple

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

#### 3.3.10 Objects

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

#### 3.3.11 Enumerations

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

The enumeration name is optional metadata for binary decoding. An unnamed Enumeration uses an empty regular String (`[0x00, 0x00, 0x00, 0x00]`). Conversion to ASON requires a valid enumeration name, either stored here or supplied by the application.

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

### 3.4 Reference Types

Reference types allow schemas to reuse type definitions, including those for Objects, Tuples, and Enumerations. Each table entry receives a zero-based index in definition order.

A type reference is a schema abbreviation, not a pointer or shared value. Its value is encoded exactly as the referenced type, with no reference index in the value area. ASONB supports value data only: object identity, shared storage, and pointer values are not represented.

The table is optional, even when complex types are defined inline. An empty table has a count of zero. All reference indices must be less than the table count. Forward references may reuse later definitions, but reference dependencies must be acyclic; recursive or mutually recursive type definitions are not supported.

#### 3.4.1 Reference Type Definition

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

### 3.5 Conversion to and from ASON

The [ASON standard](https://github.com/hemashushu/ason/blob/main/docs/specification.md) governs conversion between ASON text and ASONB. The binary format does not extend ASON's text syntax or relax its value constraints.

Conversion preserves values, not source formatting, comments, or the choice of binary encoding form. Converting from ASON requires a schema supplied by the application or derived from the value where sufficient type information is available; empty collections may require explicit element, key, or value types.

When converting to ASON, values must be representable under the applicable ASON standard. This includes valid field and variant identifiers, enumeration names, tuple and variant arities, and DateTime ranges, precision, and offsets. A converter must report an error when a value cannot be represented without loss, unless the application explicitly supplies a conversion policy. It must not silently discard precision or invent missing names.

Fixed-capacity values require application-defined interpretation before conversion, including how to identify logical content and unused slots. ASONB does not impose those low-level conventions.

## 4 File Extension and MIME Type

The file extension for ASONB documents is `.asonb`, and the MIME type is `application/asonb`.

## 5 Comparison with Other Binary Serialization Formats

These formats make different trade-offs; none is uniformly smaller or faster for every workload.

| Format | Relationship to ASONB |
|--------|-----------------------|
| BSON | Stores field names with each document and provides a different value type system. ASONB stores Object field names in the schema rather than repeating them for every value of that type. |
| Protocol Buffers | Uses numbered fields and schema-based messages, with established schema-evolution rules. ASONB embeds its schema in each document and represents ASON's value types directly. |
| Avro | Also separates schemas from value encodings and supports schema resolution. ASONB uses its own type system and provides multi-chunk String, Byte Data, and List encodings. |
| CBOR | Uses per-value type information, supports tags, and offers indefinite-length encodings. ASONB instead uses a document schema to describe value layouts. |
| MessagePack | Uses per-value type information and supports binary and extension types. ASONB describes repeated structures through a schema and defines multi-chunk encodings for incremental production. |

ASONB's focus is schema-described ASON values, forward-only streaming, and predictable offsets for fixed-width layouts. Variable-width layouts still require scanning or an external index. Storage size, access speed, and conversion cost should be compared using representative data and implementations rather than inferred from the format alone.

## 6 Links

- [ASON Specification](https://github.com/hemashushu/ason/blob/main/docs/specification.md)
- [ASON Rust Library](https://github.com/hemashushu/ason)

## 7 License

This project is licensed under the MPL 2.0 License with additional terms. See [LICENSE](./LICENSE) and [LICENSE.additional](./LICENSE.additional).
