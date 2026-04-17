# ASONB

_ASONB_ is a binary representation of ASON optimized for efficient storage, access, and transmission.

ASONB supports incremental updates, streaming, and random access, making it suitable for use cases ranging from simple persistence to complex data-processing pipelines.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [1 Features](#1-features)
- [2 Library and APIs](#2-library-and-apis)
  - [2.1 Deserialization and Serialization](#21-deserialization-and-serialization)
  - [2.2 Streaming Deserialization and Serialization](#22-streaming-deserialization-and-serialization)
- [3 Specification](#3-specification)
  - [3.1 Encoding](#31-encoding)
    - [3.1.1 Values](#311-values)
    - [3.1.2 Fixed-size Values, User-defined Values, and Raw Format](#312-fixed-size-values-user-defined-values-and-raw-format)
    - [3.1.3 Streaming and Random Access](#313-streaming-and-random-access)
    - [3.1.4 Alignment](#314-alignment)
  - [3.2 Document](#32-document)
    - [3.2.1 Document Header](#321-document-header)
    - [3.2.2 Definitions](#322-definitions)
    - [3.2.3 Data Sections](#323-data-sections)
  - [3.3 Data Types](#33-data-types)
    - [3.3.1 Integers](#331-integers)
    - [3.3.2 Unsigned Integers](#332-unsigned-integers)
    - [3.3.3 Floating Point Numbers](#333-floating-point-numbers)
    - [3.3.4 Boolean Values](#334-boolean-values)
    - [3.3.5 Characters](#335-characters)
    - [3.3.6 DateTime](#336-datetime)
    - [3.3.7 Strings](#337-strings)
    - [3.3.8 Byte Data](#338-byte-data)
    - [3.3.9 Objects](#339-objects)
    - [3.3.10 Tuples](#3310-tuples)
    - [3.3.11 Lists](#3311-lists)
    - [3.3.12 Named Lists](#3312-named-lists)
    - [3.3.13 Enumerations](#3313-enumerations)
  - [3.4 User-defined Values](#34-user-defined-values)
    - [3.4.1 User-defined String](#341-user-defined-string)
    - [3.4.2 User-defined Byte Data](#342-user-defined-byte-data)
    - [3.4.3 User-defined Objects](#343-user-defined-objects)
    - [3.4.4 User-defined Tuples](#344-user-defined-tuples)
    - [3.4.5 User-defined Lists](#345-user-defined-lists)
    - [3.4.6 User-defined Named Lists](#346-user-defined-named-lists)
    - [3.4.7 User-defined Enumerations](#347-user-defined-enumerations)
  - [3.5 Individual User-defined Values](#35-individual-user-defined-values)
  - [3.6 Reference Values](#36-reference-values)
    - [3.6.1 Reference String](#361-reference-string)
    - [3.6.2 Reference Byte Data](#362-reference-byte-data)
  - [3.7 Discrete Padding Bytes](#37-discrete-padding-bytes)
  - [3.8 File Extension and MIME Type](#38-file-extension-and-mime-type)
- [4 Notes for Implementations](#4-notes-for-implementations)
- [5 Comparison with Other Binary Serialization Formats](#5-comparison-with-other-binary-serialization-formats)
- [6 Linking](#6-linking)
- [7 License](#7-license)

<!-- /code_chunk_output -->

## 1 Features

- Streamable: ASONB supports forward-only streaming for efficient producer/consumer pipelines.

- Large-data support: ASONB handles large single values (for example, long strings and large byte arrays) and large collections (for example, lists with millions of items), making it suitable for AI training data and databases.

- Efficient access: ASONB supports memory mapping and random access without intermediate parsing (zero-copy access), enabling high-performance storage and retrieval.

- Appendable: ASONB supports incremental updates without rewriting existing file data, making it practical for logs.

## 2 Library and APIs

The Rust [asonb](https://github.com/hemashushu/asonb) library is the reference implementation of ASONB, you can add it as a dependency in your Rust project by command:

```sh
cargo add asonb
```

or by adding the following line to your `Cargo.toml` file:

```toml
[dependencies]
asonb = "1.0.0"
```

The Rust `asonb` library provides two set APIs:

1. [Serde](https://github.com/serde-rs/serde) based APIs for deserialization and serialization.
2. Streaming APIs for incremental deserialization and serialization.

### 2.1 Deserialization and Serialization

Consider the following ASON document:

```json5
{
    name: "foo"
    version: "0.1.0"
    dependencies: [
        "random"
        "regex"
    ]
}
```

This document consists of an object and a list. The object has `name`, `version` and `dependencies` fields, and the list has strings as elements. We can create a Rust struct corresponding to this document:

```rust
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Package {
    name: String,
    version: String,
    dependencies: Vec<String>,
}
```

The struct needs to be annotated with `Serialize` and `Deserialize` traits (which are provided by the _serde_ serialization framework) to enable serialization and deserialization.

The following code demonstrates how to serialize the `Package` struct instance into an binary data vector using the `asonb::ser::ser_to_writer` function:

```rust
// Serialize the `Package` struct instance into a binary data vector.
let package = Package {
        name: String::from("foo"),
        version: String::from("0.1.0"),
        dependencies: vec![
            String::from("random"),
            String::from("regex")
        ],
    };
let mut buf: Vec<u8> = vec![];
asonb::ser::ser_to_writer(&package, &mut buf).unwrap();
```

You can use the function `asonb::de::de_from_reader` to deserialize the ASONB binary data into a `Package` struct instance:

```rust
// Deserialize the ASONB binary data into a `Package` struct.
let package2: Package = asonb::de::de_from_reader(&mut buf.as_slice()).unwrap();

// Verify the deserialized `Package` struct.
assert_eq!(
    package,
    package2
);
```

### 2.2 Streaming Deserialization and Serialization

The `asonb::de` module also provides streaming deserialization APIs, which let you deserialize ASONB binary data streams incrementally without loading the entire document into memory. This is particularly useful for large documents or for data transmitted over a network connection or pipe.

Currently, the streaming deserialization APIs support only documents whose root value is a `List`. The list elements are deserialized and returned one by one through an iterator.

The following code demonstrates how to use the streaming serialization API to serialize a list of `Object` structs into ASONB binary data and write it to `stdout`:

```rust
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Object {
    id: i32,
    name: String,
}

let o1 = Object {
    id: 11,
    name: "foo".to_owned(),
};

let o2 = Object {
    id: 13,
    name: "bar".to_owned(),
};

let mut buf = std::io::stdout().lock();
let mut ser = asonb::ser::list_to_writer(&mut buf);

ser.start_list().unwrap();
ser.serialize_element(&o1).unwrap();
ser.serialize_element(&o2).unwrap();
ser.end_list().unwrap();

buf.flush().unwrap();
```

You can use the streaming deserialization API to read the ASONB binary data and deserialize the list of `Object` structs incrementally:

```rust
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Object {
    id: i32,
    name: String,
}


let mut buf = std::io::stdin().lock();
let mut char_iter = UTF8CharIterator::new(&mut buf);
let mut de = ason::de::list_from_char_iterator(&mut char_iter).unwrap();

let o1: Object = de.next().unwrap().unwrap();
assert_eq!(
    o1,
    Object {
        id: 11,
        name: "foo".to_owned()
    }
);

let o2: Object = de.next().unwrap().unwrap();
assert_eq!(
    o2,
    Object {
        id: 13,
        name: "bar".to_owned()
    }
);

assert!(de.next().is_none());

println!("Deserialization successful!");
```

To run the above example, you can save the serialization code and deserialization code to two console applications respectively (for example, `stream-ser` and `stream-de`), then build them and run the two applications in a pipeline:

```sh
stream-ser | stream-de
```

If everything works correctly, the deserialization application will print "Deserialization successful!" to the console.

## 3 Specification

### 3.1 Encoding

An ASONB document consists of ASON _values_ (including primitive values and compound values) in binary format.

#### 3.1.1 Values

Each value is encoded as a _value prefix_ followed by a _value data block_ and an optional _end marker_.

```asonb
[
    value prefix (variable length),
    value data block (variable length)
    end marker (optional, for some compound values)
]
```

For example, a 32-bit signed integer value `0x42` is encoded as:

```asonb
[
    0x15, 0x00, 0x00, 0x00,     // Value prefix of `i32` type
    0x42, 0x00, 0x00, 0x00,     // The 32-bit signed integer 0x42
]
```

The value prefix includes the _base prefix_ and the _additional prefix_, where the additional prefix is optional and its content depends on the value type. The base prefix is 4 bytes long and consists of a _type byte_, an _extension modifier byte_, and two _extension bytes_:

```asonb
[
    1-byte type (`u8`),
    1-byte extension modifier (`u8`),
    2-byte extension (`u16`),
]
```

The extension bytes (`u16`) are used for additional information about the value. By default they are not used, and their value is `0x00_00`.

The extension modifier byte (`u8`) is used to change the interpretation of the extension bytes. By default it is also not used, and its value is `0x00`.

The type byte (`u8`) indicates the data type of the value and determines how to interpret the value data block.

The following table shows general type bytes:

| Type Byte | Data Type                    | Fixed-size |
|-----------|------------------------------|------------|
| 0x13      | i8                           | Yes        |
| 0x14      | i16                          | Yes        |
| 0x15      | i32                          | Yes        |
| 0x16      | i64                          | Yes        |
| 0x23      | u8                           | Yes        |
| 0x24      | u16                          | Yes        |
| 0x25      | u32                          | Yes        |
| 0x26      | u64                          | Yes        |
| 0x35      | f32                          | Yes        |
| 0x36      | f64                          | Yes        |
| 0x40      | Boolean                      | Yes        |
| 0x41      | Character                    | Yes        |
| 0x42      | DateTime                     | Yes        |
| 0x50      | String                       |            |
| 0x51      | Chunk String                 |            |
| 0x52      | Long String                  |            |
| 0x53      | Reference String             | Yes        |
| 0x60      | Byte Data                    |            |
| 0x61      | Chunk Byte Data              |            |
| 0x62      | Long Byte Data               |            |
| 0x63      | Reference Byte Data          | Yes        |
| 0x80      | Object                       |            |
| 0x81      | Name-predefined Object       |            |
| 0x90      | Tuple                        |            |
| 0xA0      | List                         |            |
| 0xA1      | Type-specified List          |            |
| 0xA2      | Incremental Typed List       |            |
| 0xB0      | Named List                   |            |
| 0xB1      | Type-specified Named List    |            |
| 0xC0-0xC3 | Enumeration                  |            |
| 0xC4-0xC7 | Name-predefined Enumeration  |            |

Some frequently used values have their own compact representations leveraging special type bytes. Some compact representations omit the value data block and some omit the additional prefix. The following table shows compact type bytes and their corresponding values:

| Type Byte | Value                       |
|-----------|-----------------------------|
| 0xD0      | f32 `NaN`                   |
| 0xD1      | f64 `NaN`                   |
| 0xD2      | Boolean `false`             |
| 0xD3      | Boolean `true`              |
| 0xD4      | Empty String                |
| 0xD5      | Empty Byte Data             |
| 0xD6      | `Option::None` variant      |
| 0xD7      | `Option::Some(...)` variant |
| 0xD8      | `Result::Ok(...)` variant   |
| 0xD9      | `Result::Err(...)` variant  |

ASONB also allows users to define the layout of values. When using these values at the document root, the following type bytes are used:

| Type Byte | Data Type                           |
|-----------|-------------------------------------|
| 0xE0      | Individual User-defined String      |
| 0xE1      | Individual User-defined Byte Data   |
| 0xE2      | Individual User-defined Object      |
| 0xE3      | Individual User-defined Tuple       |
| 0xE4      | Individual User-defined List        |
| 0xE5      | Individual User-defined Named List  |
| 0xE6      | Individual User-defined Enumeration |

The three tables above cover all valid type bytes. An access library should throw an error if it encounters an unrecognized type byte.

The upper 4 bits (most significant bits) of the type byte indicate the category of the value:

| 4-bit MSB | Category                      |
|-----------|-------------------------------|
| 0x1       | Signed Integer                |
| 0x2       | Unsigned Integer              |
| 0x3       | Floating Point Number         |
| 0x4       | Boolean, Character, DateTime  |
| 0x5       | String                        |
| 0x6       | Byte Data                     |
| 0x7       | Reserved                      |
| 0x8       | Object                        |
| 0x9       | Tuple                         |
| 0xA       | List                          |
| 0xB       | Named List                    |
| 0xC       | Enumeration                   |
| 0xD       | Compact Representation        |
| 0xE       | Individual User-Defined Value |

In addition to values, ASONB documents contain special data blocks. These also use a prefix byte, called a _special byte_ (`u8`). Special-byte values are outside the type-byte range.

The following table shows all possible special bytes:

| Special Byte | Data Type                     |
|--------------|-------------------------------|
| 0x00         | Padding byte for alignment    |
| 0xF0         | Document header               |
| 0xF1         | Name definition               |
| 0xF2         | User-defined value definition |
| 0xF3         | Data index section            |
| 0xF4         | Data block section            |
| 0xFF         | End marker                    |

#### 3.1.2 Fixed-size Values, User-defined Values, and Raw Format

Most primitive values (including Integers, Floats, Booleans, Characters, DateTime, Reference String, and Reference Byte Data) are _fixed-size_, which means the length of their value data block is constant and known in advance. For example, the value data block of `i32` is always 4 bytes long, the value data block of `f64` is always 8 bytes long.

If a fixed-size value appears inside a type-specified List, type-specified Named List, user-defined value, or individual user-defined value, the value prefix can be omitted and only the value data block is stored. This is called the _raw format_.

For example, a regular List of `i32` is represented as:

```asonb
[
    0xA0,                               // Type byte for List
    0x00,                               // Extension bytes modifier, not used in this case
    0x00_00,                            // Extension bytes, not used in this case
    [0x00_00_00_15, 0x00_00_00_11],     // `i32` 0x11 with value prefix
    [0x00_00_00_15, 0x00_00_00_13],     // `i32` 0x13 with value prefix
    [0x00_00_00_15, 0x00_00_00_17],     // `i32` 0x17 with value prefix
    ...                                 // More items
    0xFF, 0xFF, 0xFF, 0xFF,             // List end marker
]
```

> Brackets in the binary data examples in this document are for readability only, they are neither part of the actual binary data, nor do they indicate the hierarchy of the data. The actual binary data is a flat sequence of bytes.

And a type-specified List of `i32` is represented as:

```asonb
[
    0xA1,                   // Type byte for type-specified List
    0x00,                   // Extension bytes modifier, not used in this case
    0x00_15,                // Data type of items, 0x15 means `i32`
    0x00_00_00_0A,          // List length (a random number in this example)
    [
        0x00_00_00_11,      // `i32` 0x11 in raw format
        0x00_00_00_13,      // `i32` 0x13 in raw format
        0x00_00_00_17,      // `i32` 0x17 in raw format
        ...                 // More items
    ]
]
```

As shown above, the raw format is more compact than the regular format because it omits per-item value prefixes. It is especially beneficial for large collections and enables direct memory mapping and random access without intermediate parsing (zero-copy access).

> Compact representations cannot be used in raw format.

If you want to use the raw format for String, Byte Data, and compound values, you can define the layout, the size and alignment for them in the document definition section, and then use them in raw format, these are called _user-defined values_.

> All user-defined values are fixed-size values and are encoded in raw format. Note that fixed-size values can only contain other fixed-size values; an error would occur if a fixed-size value contains a variable-size value.

Since the raw format lacks the type information, a special type byte is used if you want to use a user-defined value as document root value, or to enclose a user-defined value in a regular value, these values are called _individual user-defined values_.

#### 3.1.3 Streaming and Random Access

If data is produced and consumed continuously over a pipe, the regular format is used and values are accessed in a streaming manner. For small documents (such as application configuration objects or message packets), implementations can load the whole document into memory and access values directly after deserialization. This is the most common usage pattern.

ASONB can also persist structured data (such as data tables) in files. In this scenario, access is usually non-sequential and high-performance random access is required. Fixed-size values and raw format allow direct access without intermediate parsing or deserialization.

#### 3.1.4 Alignment

All values in ASONB are 4-byte aligned (except `i64`/`u64`/`f64` values). This means the starting offset of each value (both prefix and data block) must be a multiple of 4. To satisfy this requirement, padding bytes (`0x00`) may be appended after value bytes.

For example, a 5-byte String will be followed by 3 null bytes to make the length of the value data block 8 bytes.

For 64-bit numbers (`i64`/`u64`/`f64`), they are usually required to be aligned to 8 bytes, which means that the starting offset of each 64-bit number's value data block must be a multiple of 8. To achieve this alignment, some _discrete padding bytes_ (also `0x00`) may be inserted before the value.

For user-defined values, required alignment is specified in the definition; valid alignment ranges from 4 to 512 bytes.

The details of the value layout and the padding rules for each data type are described in the following sections.

> Alignments except for 4-byte alignment are not mandatory if the document is not used for memory mapping and random access.

### 3.2 Document

An ASONB document allows exactly one root value (primitive or compound). The root is commonly an Object or List, but a String or number is also valid.

Before the root value, an ASONB document may include an optional document header, followed by zero or more definitions and data sections.

The layout of an ASONB document is:

```asonb
[
    optional document header,
    zero or more definitions,
    zero or more data sections,
    root value
]
```

#### 3.2.1 Document Header

The document header is an 8-byte metadata block with the following content:

```asonb
[
    0xF0,                           // Type byte for document header
    0x00,                           // Indicates binary file
    0x41, 0x53, 0x4F, 0x4E, 0x42,   // Magic number "ASONB"
    0x01,                           // Features and version
]
```

The upper 4 bits (most significant bits) of the features and version byte are the feature flags, and the lower 4 bits (least significant bits) are the version number, which is currently `0x01`.

The following table lists feature flags:

| Feature Bit | Description                        |
|-------------|------------------------------------|
| 0b0000      | Default                            |
| 0b0001      | Require user-defined value support |
| 0b0010      | Require reference value support    |

For example, if a document requires user-defined value support and reference value support, the features and version byte should be `0b0011_0001` (0x31).

The access library should throw an error if it encounters a document header with unsupported features or version number.

> Support for user-defined values and reference values is not mandatory for ASONB access libraries.

The document header is optional, but it should be included when storing ASONB documents as files (for format recognition and versioning), when documents are exposed through public APIs, or when any feature flags are required.

#### 3.2.2 Definitions

Definitions are used to define Object field names, Enumeration variant names, and user-defined values.

The format of name definition is:

```asonb
[
    0xF1,
    0x00,
    1-byte definition type (`u8`),
    0x00,
    4-byte number of names (`u32`),
    names (variable length)
]
```

The following table shows all possible definition types for name definition:

| Definition Type | Description               |
|-----------------|---------------------------|
| 0x80            | Object field names        |
| 0xC0            | Enumeration variant names |

Details of name definitions are described in the "Objects" and "Enumerations" sections.

Format of user-defined value definition is:

```asonb
[
    0xF2,
    0x00,
    1-byte definition type (`u8`),
    1-byte alignment (`u8`),
    definition data (variable length)
]
```

The alignment byte indicates the required alignment for the value data block of the user-defined value. Valid values are 0 or 2 to 9, representing 2^alignment bytes of alignment. For example, if the alignment byte is `0x03`, the alignment is 2^3 = 8 bytes. Thus ASONB allows alignments from 4 to 512 bytes. Alignment values other than 0 and 2–9 are invalid and should be rejected by the access library.

If the alignment byte is `0x00`, it means that the alignment is not specified and the default alignment (4 bytes) is used. If the ASONB document is not intended to be used for memory mapping and random access, the alignment byte can be set to `0x00` for simplicity.

The following table shows all possible definition types for user-defined value definition:

| Definition Type | Description              |
|-----------------|--------------------------|
| 0x50            | User-defined String      |
| 0x60            | User-defined Byte Data   |
| 0x80            | User-defined Object      |
| 0x90            | User-defined Tuple       |
| 0xA0            | User-defined List        |
| 0xB0            | User-defined Named List  |
| 0xC0            | User-defined Enumeration |

An ASONB document can have at most `65536` name definitions and `65536` user-defined value definitions. Each definition can be referenced by its index (`0` to `65535`) in values.

The details of the definitions are described in the "User-defined Values" section.

#### 3.2.3 Data Sections

Data sections store payload data for reference values (Reference String and Reference Byte Data). Each data section pair consists of a _data index section_ and a _data block section_. An ASONB document can contain multiple section pairs with the following layout:

```asonb
[
    [
        data index section 1,
        data block section 1,
    ],
    [
        data index section 2,
        data block section 2,
    ],
    ...
]
```

The details of the data sections are described in the "Reference Values" section.

### 3.3 Data Types

The following sections describe the layout of the value prefix and value data block for each data type.

#### 3.3.1 Integers

Format:

- `[0x13, 0x00, 0x00, 0x00, 1-byte data, 3-bytes signed extension]`: 8-bit signed integer
- `[0x14, 0x00, 0x00, 0x00, 2-byte data, 2-bytes signed extension]`: 16-bit signed integer
- `[0x15, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit signed integer
- `[0x16, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit signed integer

The type byte indicates the integer type. The low 4 bits encode bit width: width = `2^(type byte & 0x0F)` bits. For example, `5` in `0x15` means `2^5 = 32` bits.

The modifier byte and the extension bytes are not used; they are set to `0x00` and `0x00_00` respectively.

The integer data is stored in little-endian format.

Example of a 32-bit signed integer `0x42`:

```asonb
[
    0x15,                       // Type byte for `i32`
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_00,                    // Extension bytes, not used in this case
    0x42, 0x00, 0x00, 0x00,     // `i32` 0x42 in little-endian format
]
```

`i8` and `i16` integers are sign-extended to 32 bits. For example, a 16-bit signed integer `0x8088` is sign-extended to `0xFFFF8088`, and it is represented as:

```asonb
[
    0x14,                       // Type byte for `i16`
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_00,                    // Extension bytes, not used in this case
    0x88, 0x80, 0xFF, 0xFF,     // 0xFFFF8088 in little-endian format with sign extension
]
```

For the 64-bit integer, since the value data block is required to be 8-byte aligned, some discrete padding bytes (`0x00`) may be inserted before the value. See the "Discrete Padding Bytes" section for more details.

#### 3.3.2 Unsigned Integers

Format:

- `[0x23, 0x00, 0x00, 0x00, 1-byte data, 0x00, 0x00, 0x00]`: 8-bit unsigned integer
- `[0x24, 0x00, 0x00, 0x00, 2-byte data, 0x00, 0x00]`: 16-bit unsigned integer
- `[0x25, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit unsigned integer
- `[0x26, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit unsigned integer

Unsigned integers are stored in the same way as signed integers, but with a different type byte (`0x23` to `0x26`), and `u8` and `u16` integers are zero-extended to 32 bits.

#### 3.3.3 Floating Point Numbers

Format:

- `[0x35, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit float (IEEE 754)
- `[0x36, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit float (IEEE 754)

When encoding special floating-point values (`NaN`, "Not a Number"), the following canonical bit patterns should be used:

- f32 NaN: `0x7FC0_0000` (quiet `NaN`, zero payload)
- f64 NaN: `0x7FF8_0000_0000_0000` (quiet `NaN`, zero payload)

Or use the compact representation for `NaN`:

- `[0xD0, 0x00, 0x00, 0x00]`: f32 `NaN`
- `[0xD1, 0x00, 0x00, 0x00]`: f64 `NaN`

#### 3.3.4 Boolean Values

Format:

```asonb
[
    0x40,
    0x00,
    0x00_00,
    4-byte boolean data (`i32`)
]
```

The boolean value is represented as a 32-bit signed integer, `0` means `false`, and any non-zero value means `true`.

- `0x00000000`: `false`
- `0x00000001` - `0xFFFFFFFF`: `true`

For consistency, use `0` for `false` and `1` for `true`.

A compact representation for Boolean values can also be used:

- `[0xD2, 0x00, 0x00, 0x00]`: `false`
- `[0xD3, 0x00, 0x00, 0x00]`: `true`

#### 3.3.5 Characters

Format:

```asonb
[
    0x41,
    0x00,
    0x00_00,
    4-byte unicode code point (`u32`)
]
```

#### 3.3.6 DateTime

Format:

```asonb
[
    0x42,
    0x00,
    0x00_00,
    [
        4-byte timestamp low (`i32`),
        4-byte timestamp high (`i32`),
        4-byte nanoseconds (`u32`),
        4-byte timezone offset seconds (`i32`)
    ]
]
```

Where:

- 8-byte timestamp: `i64` (which is split into two 4-byte parts: `timestamp low` and `timestamp high`), the number of seconds since the Unix epoch (January 1, 1970), stored in little-endian format. The timestamp can be negative to represent dates before the epoch.
- 4-byte nanoseconds: `u32`, the number of nanoseconds since the last second, stored in little-endian format. Nanoseconds are always positive; the final DateTime value is calculated as `timestamp + nanoseconds / 1_000_000_000`.
- 4-byte timezone offset seconds: `i32`, the number of seconds offset from UTC, stored in little-endian format. This field is used for display purposes only. For UTC DateTime values, this field should be set to `0x00_00_00_00`.

#### 3.3.7 Strings

Format:

```asonb
[
    0x50,
    0x00,
    2-byte number of padding bytes (`u16`),
    4-byte length of string data (`u32`),
    [
        UTF-8 string data,
        null bytes
    ]
]
```

String data is encoded in UTF-8. It is recommended to append one or more null bytes (`0x00`) so the data can be mapped as a C-style null-terminated string and aligned to 4 bytes. The number of null bytes is stored in the extension bytes. The length prefix counts only the actual string data (excluding null bytes).

For example, the string "Hello" is encoded as:

```asonb
[
    0x50,                               // Type byte for String
    0x00,                               // Extension bytes modifier, not used in this case
    0x00_03,                            // Indicates there are 3 null bytes
    0x00_00_00_05,                      // String data length (5 bytes)
    [
        0x48, 0x65, 0x6C, 0x6C, 0x6F,   // "Hello" in UTF-8
        0x00, 0x00, 0x00                // Null bytes for both null-termination and alignment
    ]
]
```

In this example, "Hello" is 5 bytes long, so 3 null bytes are appended to make the data block 8 bytes long (4-byte aligned).

If the string length is already a multiple of 4, append 4 null bytes as well (except for the empty string) so the string remains null-terminated. For example, "ABCD" is encoded as:

```asonb
[
    0x50,                           // Type byte for String
    0x00,                           // Extension bytes modifier, not used in this case
    0x00_04,                        // Indicates there are 4 null bytes
    0x00_00_00_04,                  // String data length (4 bytes)
    [
        0x41, 0x42, 0x43, 0x44,     // "ABCD" in UTF-8
        0x00, 0x00, 0x00, 0x00      // Null bytes for both null-termination and alignment
    ]
]
```

##### 3.3.7.1 Chunk String

A Chunk String is encoded as a sequence of chunks, where each chunk is a fragment of the full string. The last chunk is followed by a 4-byte `0x00` end marker.

Format:

```asonb
[
    0x51,                               // Type byte for Chunk String
    0x00,                               // Extension bytes modifier, not used in this case
    0x00_00,                            // Extension bytes, not used in this case
    [                                   // The first chunk
        4-byte string length (`u32`),
        4-byte number of null bytes (`u32`),
        UTF-8 string data,
        null bytes,
    ]
    [                                   // The second chunk
        4-byte string length (`u32`),
        4-byte number of null bytes (`u32`),
        UTF-8 string data,
        null bytes,
    ]
    ...,                                // More chunks
    0x00, 0x00, 0x00, 0x00              // End marker for Chunk String
]
```

The Chunk String generator should ensure each chunk is a valid UTF-8 fragment, meaning a chunk must not split a Unicode code point.

Each chunk also needs trailing null bytes for null-termination and alignment. The number of null bytes is stored in the chunk header. The length prefix counts only the actual string data in the chunk.

Chunk Strings are useful for streaming scenarios where data is produced incrementally.

##### 3.3.7.2 Long String

If the length of a String is greater than `0xFFFF_FFFF` (4GB), the string must be encoded in the "Long String" format:

```asonb
[
    0x52,
    0x00,
    2-byte number of null bytes (`u16`),
    4-byte string length low (`u32`),
    4-byte string length high (`u32`),
    [
        UTF-8 string data,
        null bytes
    ]
]
```

##### 3.3.7.3 Empty String

An empty string can be encoded as a regular String with zero length:

```asonb
[
    0x50,                       // Type byte for String
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_00,                    // Indicates there is no null byte
    0x00, 0x00, 0x00, 0x00,     // String data length (0 byte)
]
```

A compact representation for the empty string is provided:

`[0xD4, 0x00, 0x00, 0x00]`: Empty String

#### 3.3.8 Byte Data

Byte Data represents arbitrary binary payloads, such as images, audio, and types not natively defined by ASONB (for example, fixed-point numbers, decimal numbers, and UUIDs).

Format:

```asonb
[
    0x60,
    0x00,
    2-byte number of padding bytes (`u16`),
    4-byte length of byte data (`u32`),
    [
        data bytes,
        padding bytes
    ]
]
```

For example, Byte Data `0xDE, 0xAD, 0xBE, 0xEF, 0x00` (5 bytes) is encoded as:

```asonb
[
    0x60,                               // Type byte for Byte Data
    0x00,                               // Extension bytes modifier, not used in this case
    0x00_03,                            // Indicates there are 3 padding bytes
    0x00_00_00_05,                      // Byte data length (5 bytes)
    [
        0xDE, 0xAD, 0xBE, 0xEF, 0x00,   // Byte data
        0x00, 0x00, 0x00                // Padding bytes for alignment
    ]
]
```

##### 3.3.8.1 Chunk Byte Data

Chunk Byte Data is encoded as a sequence of chunks, where each chunk is a fragment of the full Byte Data value. The last chunk is followed by a 4-byte `0x00` end marker.

```asonb
[
    0x61,                               // Data type for Chunk Byte Data
    0x00,                               // Extension bytes modifier, not used in this case
    0x00_00,                            // Extension bytes, not used in this case
    [                                   // The first chunk
        4-byte data length,
        4-byte number of padding bytes,
        data,
        padding bytes,
    ]
    [                                   // The second chunk
        4-byte data length,
        4-byte number of padding bytes,
        data,
        padding bytes,
    ]
    ... ,                               // More chunks
    0x00, 0x00, 0x00, 0x00              // End marker for Chunk Byte Data
]
```

##### 3.3.8.2 Long Byte Data

If the length of Byte Data is greater than `0xFFFF_FFFF` (4GB), the Byte Data must be encoded in the "Long Byte Data" format:

```asonb
[
    0x62,
    0x00,
    2-byte number of padding bytes (`u16`),
    4-byte byte data length low (`u32`),
    4-byte byte data length high (`u32`),
    [
        data bytes,
        padding bytes
    ]
]
```

##### 3.3.8.3 Empty Byte Data

An empty Byte Data can be encoded as a regular Byte Data with zero length:

```asonb
[
    0x60,                       // Type byte for Byte Data
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_00,                    // Indicates there is no padding byte
    0x00_00_00_00,              // Byte data length (0 byte)
]
```

A compact representation for empty Byte Data is provided:

`[0xD5, 0x00, 0x00, 0x00]`: Empty Byte Data

#### 3.3.9 Objects

Objects are sequences of key-value pairs.

Format:

```asonb
[
    0x80,
    0x00,
    0x00_00,
    key-value pairs,
    0xFF, 0xFF, 0xFF, 0xFF,
]
```

Key-value pairs are encoded sequentially without separators, followed by an end marker (`0xFF, 0xFF, 0xFF, 0xFF`) to indicate the end of the Object.

Key-value pair format:

```asonb
[
    key,
    value
]
```

The _key_ is an identifier in ASON, but in ASONB it is represented as a regular string for simplicity. The _value_ can be of any type.

For example, consider the following object:

```ason
{
    id: 0x42
    name: "Alice"
}
```

Its binary representation would be:

```asonb
[
    0x80,                       // Type byte for Object
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_00,                    // Extension bytes, not used in this case
    [                           // The key, String "id"
        0x50, 0x00, 0x00_02,
        0x00_00_00_02,
        "id", 0x00, 0x00
    ],
    [                           // The value, `i32` 0x42
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_42
    ],
    [                           // The key, String "name"
        0x50, 0x00, 0x00_04,
        0x00_00_00_04,
        "name",
        0x00, 0x00, 0x00, 0x00
    ],
    [                           // The value, String "Alice"
        0x50, 0x00, 0x00_03,
        0x00_00_00_05,
        "Alice",
        0x00, 0x00, 0x00
    ],
    0xFF, 0xFF, 0xFF, 0xFF      // Object end marker
]
```

##### 3.3.9.1 Name-predefined Object

To save space, field names can be defined in the name definition and then referenced by their indexes in the Object value.

Format:

```asonb
[
    0x81,
    0x00,
    2-byte index of definition (`u16`),
    4-byte index of field name 1 (`u32`),
    value 1,
    4-byte index of field name 2 (`u32`),
    value 2,
    ...,
    0xFF, 0xFF, 0xFF, 0xFF
]
```

To use the name-predefined Object, the field names must be defined in the name definition section. The field name definition has the following format:

```asonb
[
    0xF1,
    0x00,
    0x80,
    0x00,
    4-byte number of field names (`u32`),
    field names (variable length)
]
```

Field names are encoded as a sequence of regular String values without separators.

Consider an Object with this structure:

```ason
{
    id: i32
    name: String
}
```

The following represents the field name definition for the above structure:

```asonb
[
    0xF1,                       // Type byte for name definition
    0x00,                       // Padding
    0x80,                       // Definition type for field names (0x80)
    0x00,                       // Padding
    0x00_00_00_02,              // Number of field names, `u32` 2 in this example
    [                           // String "id"
        0x50, 0x00, 0x00_02,
        0x00_00_00_02,
        "id",
        0x00, 0x00
    ],
    [                           // String "name"
        0x50, 0x00, 0x00_04,
        0x00_00_00_04,
        "name",
        0x00, 0x00, 0x00, 0x00
    ],
]
```

Assume the above definition is at index `0x00_01`, then Object `{id: 0x42, name: "Alice"}` can be encoded as:

```asonb
[
    0x81,                       // Type byte for Name-predefined Object
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_01,                    // Index of the definition (0x00_01 in this example)
    [0x00_00_00_00],            // Index of field name "id", `u32` 0
    [                           // The value `i32` 0x42
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_42
    ],
    [0x00_00_00_01],            // Index of field name "name", `u32` 1
    [                           // The value "Alice"
        0x50, 0x00, 0x00_03,
        0x00_00_00_05,
        "Alice",
        0x00, 0x00, 0x00
    ],
    0xFF, 0xFF, 0xFF, 0xFF      // Object end marker
]
```

Note that the field name definition only contains the names; the types of the values are not included. Thus the values are still encoded in regular format with value prefixes.

#### 3.3.10 Tuples

Tuples are collections of values where each value can be of any type.

Format:

```asonb
[
    0x90,
    0x00,
    0x00_00,
    values,
    0xFF, 0xFF, 0xFF, 0xFF
]
```

Values are encoded sequentially without separators, followed by an end marker (`0xFF, 0xFF, 0xFF, 0xFF`) to indicate the end of the Tuple.

For example, consider the following Tuple:

```ason
(0x2B, "foo", true)
```

Its binary representation would be:

```asonb
[
    0x90,                           // Data type for Tuple
    0x00,                           // Extension bytes modifier, not used in this case
    0x00_00,                        // Extension bytes, not used in this case
    [                               // The 32-bit signed integer 0x2B
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_2B
    ],
    [                               // The String "foo"
        0x50, 0x00, 0x00_01,
        0x00_00_00_03,
        "foo",
        0x00,
    ],
    [                               // The Boolean value `true`
        0xD3, 0x00, 0x00, 0x00
    ],
    0xFF, 0xFF, 0xFF, 0xFF          // Tuple end marker
]
```

#### 3.3.11 Lists

Lists are sequences of values of the same type.

Format:

```asonb
[
    0xA0,
    0x00,
    0x00_00,
    items,
    0xFF, 0xFF, 0xFF, 0xFF
]
```

Items are encoded sequentially without separators, followed by an end marker (`0xFF, 0xFF, 0xFF, 0xFF`) to indicate the end of the List.

For example, consider the following List of `i32` integers:

```ason
[0x11, 0x13, 0x17, 0x19]
```

Its binary representation would be:

```asonb
[
    0xA0,                       // Type byte for List
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_00,                    // Extension bytes, not used in this case
    [                           // The first item, `i32` 0x11
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_11
    ],
    [                           // The second item, `i32` 0x13
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_13
    ],
    [                           // The third item, `i32` 0x17
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_17
    ],
    [                           // The fourth item, `i32` 0x19
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_19
    ],
    0xFF, 0xFF, 0xFF, 0xFF      // List end marker
]
```

##### 3.3.11.1 Type-specified Lists

Type-specified Lists are Lists with a specified data type for items. Values are encoded in raw format, eliminating the need for value prefixes for each item, thus making them more compact than regular Lists.

Only fixed-size values (i.e., fixed-size primitive values and user-defined values) can be used as items in type-specified Lists, variable-length values (such as regular String and Byte Data) are not allowed.

The format of type-specified List is:

```asonb
[
    0xA1,
    1-byte data type modifier (`u8`),
    2-byte data type (`u16`),
    4-byte list length (`u32`),
    items
]
```

Data type modifier is used to change the interpretation of the data type field:

- If the data type of items is primitive fixed-size type, the data type modifier should be set to `0x00`, and the data type is determined by the 2-byte data type field. For example, for `i32` integers, the data type is `0x00_15` (the type byte for `i32` is `0x15`).
- If the data type of items is user-defined value, the data type modifier should be set to `0x01`, and the data type is determined by the index of user-defined value definition.

Let's rewrite the previous example List of `i32` integers `[0x11, 0x13, 0x17, 0x19]` with type-specified List:

```asonb
[
    0xA1,                       // Type byte for type-specified List
    0x00,                       // Data type modifier, set to 0x00 for fixed-size primitive types
    0x00_15,                    // Data type for items, in this case it is `i32`
    0x00_00_00_04,              // List length (4 items in this example)
    [
        0x00_00_00_11,          // `i32` 0x11
        0x00_00_00_13,          // `i32` 0x13
        0x00_00_00_17,          // `i32` 0x17
        0x00_00_00_19,          // `i32` 0x19
    ]
]
```

##### 3.3.11.2 Incremental Type-specified Lists

Incremental type-specified Lists are type-specified Lists without the list length field; the end of the list is indicated by the end of document (or EOF). Incremental type-specified Lists are only allowed as the root value of an ASONB document.

Format:

```asonb
[
    0xA2,
    1-byte data type modifier (`u8`),
    2-byte data type (`u16`),
    items,
    EOF
]
```

#### 3.3.12 Named Lists

Named Lists are similar to Lists, but each value is associated with a name. The name is usually a string or number, but it can be any type of value.

Format:

```asonb
[
    0xB0,
    0x00,
    0x00_00,
    name-value pairs,
    0xFF, 0xFF, 0xFF, 0xFF
]
```

Name-value pairs are encoded sequentially without separators, followed by an end marker (`0xFF, 0xFF, 0xFF, 0xFF`) to indicate the end of the Named List.

The format of name-value pair is:

```asonb
[
    name_data,
    value_data
]
```

For example, consider this Named List:

```ason
[
    "Red": 0xff0000,
    "Green": 0x00ff00,
    "Blue": 0x0000ff
]
```

Its binary representation would be:

```asonb
[
    0xB0,                           // Data type for named list
    0x00,                           // Extension bytes modifier, not used in this case
    0x00_00,                        // Extension bytes, not used in this case
    [                               // The name "Red"
        0x50, 0x00, 0x00_01,
        0x00_00_00_03,
        "Red",
        0x00
    ],
    [                               // The value 0xff0000
        0x15, 0x00, 0x00, 0x00,
        0x00_ff_00_00
    ],
    [                               // The name "Green"
        0x50, 0x00, 0x00_03,
        0x00_00_00_05,
        "Green",
        0x00, 0x00, 0x00
    ],
    [                               // The value 0x00ff00
        0x15, 0x00, 0x00, 0x00,
        0x00_00_ff_00
    ],
    [                               // The name "Blue"
        0x50, 0x00, 0x00_04,
        0x00_00_00_04,
        "Blue",
        0x00, 0x00, 0x00, 0x00
    ],
    [                               // The value 0x0000ff
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_ff
    ],
    0xFF, 0xFF, 0xFF, 0xFF          // Named List end marker
]
```

##### 3.3.12.1 Type-specified Named Lists

Type-specified Named Lists are Named Lists with a specified data type for keys and values; both names and values are encoded in raw format.

Only fixed-size values can be used as keys and values in type-specified Named Lists, variable-length values (such as regular String) are not allowed.

Format:

```asonb
[
    0xB1,
    1-byte value data type modifier (`u8`),
    2-byte value data type (`u16`),
    0x00,
    1-byte key data type modifier (`u8`),
    2-byte key data type (`u16`),
    4-byte list length (`u32`),
    name-value pairs
]
```

#### 3.3.13 Enumerations

Enumerations represent a custom data type that consists of a type name and a set of named variants. A variant can have associated data, which can be of any type.

There are four kinds of variants in ASON:

- Variant without value (e.g. `Option::None`)
- Variant with single value (e.g. `Option::Some(123)`)
- Object-like variant (e.g. `Shape::Rectangle { width: 10, height: 20 }`)
- Tuple-like variant (e.g. `Color::RGB(255_u8, 127_u8, 63_u8)`)

##### 3.3.13.1 Variant without value

A variant without value is the simplest kind of variant; it only contains the type name and the variant name.

Format:

```asonb
[
    0xC0,
    0x00,
    0x00_00,
    type name,
    variant name
]
```

Type name and variant name are identifiers in ASON, but in ASONB they are represented as regular strings for simplicity.

For example, consider this enumeration:

```ason
Option::None
```

Its binary representation would be:

```asonb
[
    0xC0,                   // Data type for variant without value
    0x00,                   // Extension bytes modifier, not used in this case
    0x00_00,                // Extension bytes, not used in this case
    [                       // The type name "Option"
        0x50,
        0x00,
        0x00_02,
        0x00_00_00_06,
        "Option",
        0x00, 0x00
    ],
    [                       // The variant name "None"
        0x50,
        0x00,
        0x00_04,
        0x00_00_00_04,
        "None",
        0x00, 0x00, 0x00, 0x00
    ]
]
```

##### 3.3.13.2 Variant with single value

A variant with a single value can carry a single value of any type.

Format:

```asonb
[
    0xC1,
    0x00,
    0x00_00,
    type name,
    variant name,
    value
]
```

For example, consider the following enumeration:

```ason
Option::Some(0x42)                              // Integer value
Option::Some("Hello")                           // String value
Option::Some({id: 0x42, name: "Alice"})         // Object value
Option::Some((0x2B, "foo", true))               // Tuple value
Option::Some([0x11, 0x13, 0x17, 0x19])          // List value
```

Their binary representation would be:

- Variant with integer value

```asonb
[
    0xC1,                                       // Data type for variant with single value
    0x00,                                       // Extension bytes modifier, not used in this case
    0x00_00,                                    // Extension bytes, not used in this case
    [
        0x50, 0x00, 0x00_02, 0x00_00_00_06,
        "Option", 0x00, 0x00
    ],
    [
        0x50, 0x00, 0x00_04, 0x00_00_00_04,
        "Some", 0x00, 0x00, 0x00, 0x00
    ],
    [                                           // The 32-bit signed integer 0x42
        0x15, 0x00, 0x00_00, 0x00_00_00_42
    ],
]
```

- Variant with String value

```asonb
[
    0xC1,                                       // Data type for variant with single value
    0x00,                                       // Extension bytes modifier, not used in this case
    0x00_00,                                    // Extension bytes, not used in this case
    [
        0x50, 0x00, 0x00_02, 0x00_00_00_06,
        "Option", 0x00, 0x00
    ],
    [
        0x50, 0x00, 0x00_04, 0x00_00_00_04,
        "Some", 0x00, 0x00, 0x00, 0x00
    ],
    [                                           // The String value "Hello"
        0x50, 0x00, 0x00_03, 0x00_00_00_05,
        "Hello", 0x00, 0x00, 0x00
    ],
]
```

- Variant with Object value

```asonb
[
    0xC1,                                       // Data type for variant with single value
    0x00,                                       // Extension bytes modifier, not used in this case
    0x00_00,                                    // Extension bytes, not used in this case
    [
        0x50, 0x00, 0x00_02, 0x00_00_00_06,
        "Option", 0x00, 0x00
    ],
    [
        0x50, 0x00, 0x00_04, 0x00_00_00_04,
        "Some", 0x00, 0x00, 0x00, 0x00
    ],
    [
        0x80,                                   // Data type for Object
        0x00,                                   // Extension bytes modifier, not used in this case
        0x00_00,                                // Extension bytes, not used in this case
        [                                       // The key "id"
            0x50, 0x00, 0x00_02, 0x00_00_00_02,
            "id", 0x00, 0x00
        ],
        [                                       // The value `i32` 0x42
            0x15, 0x00, 0x00_00, 0x00_00_00_42
        ],
        [                                       // The key "name"
            0x50, 0x00, 0x00_04, 0x00_00_00_04,
            "name", 0x00, 0x00, 0x00, 0x00
        ],
        [                                       // The value "Alice"
            0x50, 0x00, 0x00_03, 0x00_00_00_05,
            "Alice", 0x00, 0x00, 0x00
        ],
        0xFF, 0xFF, 0xFF, 0xFF                  // Object end marker
    ],
]
```

- Variant with Tuple value

```asonb
[
    0xC1,                                       // Data type for variant with single value
    0x00,                                       // Extension bytes modifier, not used in this case
    0x00_00,                                    // Extension bytes, not used in this case
    [
        0x50, 0x00, 0x00_02, 0x00_00_00_06,
        "Option", 0x00, 0x00
    ],
    [
        0x50, 0x00, 0x00_04, 0x00_00_00_04,
        "Some", 0x00, 0x00, 0x00, 0x00
    ],
    [
        0x90,                                   // Data type for Tuple
        0x00,                                   // Extension bytes modifier, not used in this case
        0x00_00,                                // Extension bytes, not used in this case
        [                                       // The 32-bit signed integer 0x2B
            0x15, 0x00, 0x00_00, 0x00_00_00_2B
        ],
        [                                       // The String "foo"
            0x50, 0x00, 0x00_01, 0x00_00_00_03,
            "foo", 0x00
        ],
        [                                       // The Boolean value `true`
            0xD3, 0x00, 0x00, 0x00
        ],
        0xFF, 0xFF, 0xFF, 0xFF                  // Tuple end marker
    ]
]
```

- Variant with List value

```asonb
[
    0xC1,                                       // Data type for variant with single value
    0x00,                                       // Extension bytes modifier, not used in this case
    0x00_00,                                    // Extension bytes, not used in this case
    [
        0x50, 0x00, 0x00_02, 0x00_00_00_06,
        "Option", 0x00, 0x00
    ],
    [
        0x50, 0x00, 0x00_04, 0x00_00_00_04,
        "Some", 0x00, 0x00, 0x00, 0x00
    ],
    [
        0xA1,                                   // Data type for Type-specified List
        0x00,                                   // Extension bytes modifier, set to 0x00 for fixed-size primitive types
        0x00_15,                                // Data type for items, in this case it is `i32`
        0x00_00_00_04,                          // List length (4 items in this example)
        0x00_00_00_11,                          // The first item `i32` 0x11
        0x00_00_00_13,                          // The second item `i32` 0x13
        0x00_00_00_17,                          // The third item `i32` 0x17
        0x00_00_00_19,                          // The fourth item `i32` 0x19
    ]
]
```

##### 3.3.13.3 `Option` and `Result` Variants

For the common `Option` and `Result` enumerations, there are compact representations for their variants:

- `Option::None`: `[0xD6, 0x00, 0x00, 0x00]`
- `Option::Some(value)`: `[0xD7, 0x00, 0x00, 0x00, value]`
- `Result::Ok(value)`: `[0xD8, 0x00, 0x00, 0x00, value]`
- `Result::Err(value)`: `[0xD9, 0x00, 0x00, 0x00, value]`

##### 3.3.13.4 Object-like variant

Format:

```asonb
[
    0xC2,
    0x00,
    0x00_00,
    type name,
    variant name,
    object value
]
```

Consider this enumeration:

```ason
User::Local { id: 0x42, name: "Alice" }
```

Its binary representation would be:

```asonb
[
    0xC2,                                       // Data type for object-like variant
    0x00,                                       // Extension bytes modifier, not used in this case
    0x00_00,                                    // Extension bytes, not used in this case
    [
        0x50, 0x00, 0x00_04, 0x00_00_00_04,
        "User", 0x00, 0x00, 0x00, 0x00
    ]
    [
        0x50, 0x00, 0x00_03, 0x00_00_00_05,
        "Local", 0x00, 0x00, 0x00
    ],
    [
        0x80,                                   // Data type for Object
        0x00,                                   // Extension bytes modifier, not used in this case
        0x00_00,                                // Extension bytes, not used in this case
        [                                       // The key "id"
            0x50, 0x00, 0x00_02, 0x00_00_00_02,
            "id", 0x00, 0x00
        ],
        [                                       // The value `i32` 0x42
            0x15, 0x00, 0x00_00, 0x00_00_00_42
        ],
        [                                       // The key "name"
            0x50, 0x00, 0x00_04, 0x00_00_00_04,
            "name", 0x00, 0x00, 0x00, 0x00
        ],
        [                                       // The value "Alice"
            0x50, 0x00, 0x00_03, 0x00_00_00_05,
            "Alice", 0x00, 0x00, 0x00
        ],
        0xFF, 0xFF, 0xFF, 0xFF                  // Object end marker
    ]
]
```

You may notice that the Object-like variant is very similar to the variant with a single Object value. The only difference is the data type byte (`0xC2` for Object-like variant and `0xC1` for variant with single value); the encoding format of the Object value is the same in both cases. This is by design.

##### 3.3.13.5 Tuple-like variant

Format:

```asonb
[
    0xC3,
    0x00,
    0x00_00,
    type name,
    variant name,
    tuple value
]
```

The binary representation of a tuple-like variant is similar to the variant with a single Tuple value. The only difference is the data type byte (`0xC3` for tuple-like variant and `0xC1` for variant with single value).

##### 3.3.13.6 Name-predefined Variants

To save space, variant names can be defined in the name definition and then referenced by their indexes in the variant value.

Format:

```asonb
[
    0xC4|0xC5|0xC6|0xC7,
    0x00,
    2-byte index of definition (`u16`),
    type name,
    4-byte index of variant name(`u32`),
    value
]
```

To use the name-predefined variant, the variant names must be defined in the name definition section. The variant name definition has the following format:

```asonb
[
    0xF1,
    0x00,
    0xC0,
    0x00,
    4-byte number of variant names (`u32`),
    type name,
    variant names (variable length)
]
```

Variant names are encoded as a sequence of regular String values without separators.

Consider the following enumeration:

```ason
enum User {
    Local { id: i32, name: String },
    Remote { url: String }
}
```

The following represents the variant name definition for the above enumeration:

```asonb
[
    0xF1,                               // Type byte for name definition
    0x00,
    0xC0,                               // Definition type for variant names (0xC0)
    0x00,
    0x00_00_00_02,                      // Number of variant names, `u32` 2 in this example
    [                                   // The type name "User"
        0x50, 0x00, 0x00_04, 0x00_00_00_04,
        "User", 0x00, 0x00, 0x00, 0x00
    ],
    [                                   // The variant name "Local"
        0x50, 0x00, 0x00_03, 0x00_00_00_05,
        "Local", 0x00, 0x00, 0x00
    ],
    [                                   // The variant name "Remote"
        0x50, 0x00, 0x00_02, 0x00_00_00_06,
        "Remote", 0x00, 0x00
    ],
]
```

Assume the above definition is at index `0x00_01`, then the variant `User::Local { id: 0x42, name: "Alice" }` can be encoded as:

```asonb
[
    0xC6,                                       // Type byte for name-predefined object-like variant
    0x00,                                       // Extension bytes modifier, not used in this case
    0x00_01,                                    // Index of the definition (0x00_01 in this example)
    [0x00_00_00_00],                            // Index of the variant name "Local", `u32` 0
    [                                           // The Object value for the variant
        ...
    ]
]
```

Name-predefined variants eliminate the type name and variant name from the variant value, making them more compact than regular variants.

### 3.4 User-defined Values

You can define a value layout (such as length, alignment, fields, and data type) in the definition section, then reference that definition in values. This allows raw-format encoding without value prefixes, which is more compact than regular encoding.

Support for user-defined values is optional for ASONB access libraries. If a document requires user-defined values, include a document header and set feature flag `0b0001`.

The following sections describe the layout of user-defined value definitions.

#### 3.4.1 User-defined String

A user-defined String has a fixed capacity (i.e., the maximum length). The actual string data is padded with null bytes to fill the capacity. The capacity is defined in the definition.

Format:

```asonb
[
    0xF2,
    0x00,
    0x50,
    0x00,
    capacity (`u32`),
]
```

For example, the following definition defines a user-defined String with a capacity of 16 bytes:

```asonb
[
    0xF2,               // Type byte for user-defined value definition
    0x00,               // Padding
    0x50,               // Definition type for user-defined String
    0x00,               // Alignment, not used in this case
    0x00_00_00_10,      // Capacity of the user-defined String, `u32` 16
]
```

Assume the above definition is at index `0x00_01`. The value of this user-defined String can then be encoded in raw format without a value prefix. For example, the following shows a type-specified List of this user-defined String:

```asonb
[
    0xA1,               // Type byte for type-specified List
    0x01,               // Extension bytes modifier, set to `0x01` in this case
    0x00_01,            // Index of the user-defined String definition, `0x00_01` in this example
    0x00_00_00_0A,      // List length (a random value in this example)
    [
        "Hello...",     // String data, padded with null bytes to fill the capacity
        "World...",     // String data, padded with null bytes to fill the capacity
        ...             // More items
    ]
]
```

Notes for user-defined strings:

- The capacity should be a multiple of 4.
- The definition specifies only capacity, not the actual string length. Actual length can be determined by finding the first null byte or by storing length in a separate value.

#### 3.4.2 User-defined Byte Data

A user-defined Byte Data has a fixed capacity. The actual data is padded to fill the capacity. The capacity is defined in the definition.

Format:

```asonb
[
    0xF2,
    0x00,
    0x60,
    1-byte alignment (`u8`),
    capacity (`u32`),
]
```

For example, the following definition defines a user-defined Byte Data with a capacity of 16 bytes and 4-byte alignment:

```asonb
[
    0xF2,                       // Type byte for user-defined value definition
    0x00,                       // Padding
    0x60,                       // Definition type for user-defined Byte Data
    0x02,                       // Alignment, set to 0x02 for 4-byte alignment
    0x00_00_00_10,              // Capacity of the user-defined Byte Data, `u32` 16
]
```

Notes for user-defined Byte Data:

- The capacity should be a multiple of 4.
- The definition only defines the capacity of the Byte Data; it does not define the actual length of the Byte Data.

#### 3.4.3 User-defined Objects

If an Object has a fixed layout (i.e., the field names, the data types, and the sizes of values are all specified), all its fields can be encoded in raw format without field names and value prefixes.

The user-defined Object definition has the following format:

```asonb
[
    0xF2,
    0x00,
    0x80,
    1-byte alignment (`u8`),
    4-byte number of field definitions (`u32`),
    field definitions
]
```

The 1-byte alignment indicates the required alignment for the data block of the Object. Its value is equal to the maximum alignment of all fields. For example, if an Object has two fields, one is `i32` (4-byte alignment) and the other is `f64` (8-byte alignment), then the alignment byte should be set to `0x03` for 8-byte alignment.

The field definitions are a sequence of field definitions without any separator. Each field definition has the following format:

```asonb
[
    0x00,
    1-byte data type modifier (`u8`),
    2-byte data type (`u16`),
    field name
]
```

Where the field name is a regular String value.

For example, consider an Object with this layout:

```ason
{
    id: i32
    name: 32-byte length String
}
```

Assuming we have already defined a user-defined String with a capacity of 32 bytes, and its definition index is `0x00_01`, then we can define the user-defined Object as:

```asonb
[
    0xF2,                       // Type byte for user-defined value definition
    0x00,                       // Padding
    0x80,                       // Definition type for user-defined Object (0x80)
    0x00,                       // Alignment, 0 for default alignment (4 bytes)
    0x00_00_00_02,              // Number of field definitions, `u32` 2 in this example
    [                           // The first field
        0x00,                   // Padding
        0x00,                   // Data type modifier, set to `0x00` for fixed-size primitive types
        0x00_15,                // Data type of the field value, in this case it is `i32`
        [                       // String "id"
            0x50, 0x00, 0x00_02,
            0x00_00_00_02,
            "id",
            0x00, 0x00
        ]
    ],
    [                           // The second field
        0x00,                   // Padding
        0x01,                   // Data type modifier, set to `0x01` for user-defined value
        0x00_01,                // Index of the user-defined String definition (0x00_01 in this example)
        [                       // String "name"
            0x50, 0x00, 0x00_04,
            0x00_00_00_04,
            "name",
            0x00, 0x00, 0x00, 0x00
        ]
    ],
]
```

Assuming the above definition is at index `0x00_02`, then the value of this user-defined Object can be encoded in raw format without value prefix. For example, the following demonstrates a type-specified List of this user-defined Object:

```ason
[
    {id: 0x42, name: "Alice"},
    {id: 0x43, name: "Bob"},
]
```

Its binary representation would be:

```asonb
[
    0xA1,                       // Type byte for type-specified List
    0x01,                       // Data type modifier, set to 0x01 for user-defined values
    0x00_02,                    // Data type for items, index of the user-defined Object definition (0x00_02 in this example)
    0x00_00_00_02,              // List length (2 items in this example)
    [
        [                       // The first item
            0x00_00_00_42,      // The value of field "id" (i32 0x42) in raw format
            "Alice...",         // The value of field "name" (String "Alice") in raw format with null bytes
        ],
        [                       // The second item
            0x00_00_00_43,      // The value of field "id" (i32 0x43) in raw format
            "Bob...",           // The value of field "name" (String "Bob") in raw format with null bytes
        ],
    ]
]
```

Internal padding bytes may be inserted between fields to satisfy alignment requirements. Unlike C struct layout optimization, field order in user-defined Objects is fixed and must not be rearranged.

> User-defined Objects in raw format are significantly more compact than the regular Object values.

#### 3.4.4 User-defined Tuples

Format:

```asonb
[
    0xF2,
    0x00,
    0x90,
    1-byte alignment (`u8`),
    4-byte number of elements (`u32`),
    element data types
]
```

The value of alignment byte is equal to the maximum alignment of all elements in the Tuple.

The element data types are a sequence of data types without any separator. Each data type consists of 4 bytes:

```asonb
[
    0x00,
    1-byte data type modifier (`u8`),
    2-byte data type (`u16`),
]
```

For example, consider a Tuple with this layout:

```ason
(
    i32, 32-byte length String, bool
)
```

Assuming we have already defined a user-defined String with a capacity of 32 bytes, and its definition index is `0x00_01`, then we can define the user-defined Tuple as:

```asonb
[
    0xF2,                       // Type byte for user-defined value definition
    0x00,                       // Padding
    0x90,                       // Definition type for user-defined Tuple (0x90)
    0x00,                       // Alignment, 0 for default alignment (4 bytes)
    0x00_00_00_03,              // Number of elements, `u32` 3 in this example
    [                           // The first element
        0x00,                   // Padding
        0x00,                   // Data type modifier, set to `0x00` for fixed-size primitive types
        0x00_15,                // Data type of the element value, in this case it is `i32`
    ],
    [                           // The second element
        0x00,                   // Padding
        0x01,                   // Data type modifier, set to `0x01` for user-defined value
        0x00_01,                // Index of the user-defined String definition (0x00_01 in this example)
    ],
    [                           // The third element
        0x00,                   // Padding
        0x00,                   // Data type modifier, set to `0x00` for fixed-size primitive types
        0x00_40,                // Data type of the element value, in this case it is `Boolean`
    ],
]
```

#### 3.4.5 User-defined Lists

A user-defined List is a List with a predefined length.

Format:

```asonb
[
    0xF2,
    0x00,
    0xA0,
    1-byte alignment (`u8`),
    0x00,
    1-byte data type modifier (`u8`),
    2-byte data type (`u16`),
    4-byte list length (`u32`),
]
```

#### 3.4.6 User-defined Named Lists

A user-defined Named List is a Named List with a predefined fixed length.

Format:

```asonb
[
    0xF2,
    0x00,
    0xB0,
    1-byte alignment (`u8`),
    0x00,
    1-byte value data type modifier (`u8`),
    2-byte value data type (`u16`),
    0x00,
    1-byte key data type modifier (`u8`),
    2-byte key data type (`u16`),
    4-byte list length (`u32`),
]
```

#### 3.4.7 User-defined Enumerations

A user-defined Enumeration is an Enumeration with predefined variants.

Format:

```asonb
[
    0xF2,
    0x00,
    0xC0,
    1-byte alignment (`u8`),
    4-byte number of variants (`u32`),
    type name,
    variant definitions
]
```

Variant definitions are a sequence of variant definitions without any separator. Each variant definition has the following format:

- Variant without value

```asonb
[
    0x00,
    0x00,
    0x00_00,
    variant name
]
```

- Variant with single value

```asonb
[
    0x01,
    1-byte variant type modifier (`u8`),
    2-byte variant type (`u16`),
    variant name
]
```

- Object-like variant

```asonb
[
    0x02,
    0x01,
    2-byte user-defined Object definition index (`u16`),
    variant name
]
```

- Tuple-like variant

```asonb
[
    0x03,
    0x01,
    2-byte user-defined Tuple definition index (`u16`),
    variant name
]
```

Note that the size of a user-defined Enumeration value is fixed, and it is determined by the maximum size of its variants plus an extra `u32` used to store the variant index. For example, consider the following Enumeration:

```ason
enum Color {
    White,
    Gray(u8),
    RGBA(u8, u8, u8, u8),
}
```

The size of `Color` value is 20 bytes:

- The variant `RGBA` has the largest size among all variants, it occupies 16 bytes (each `u8` actually occupies 4 bytes in raw format).
- An extra 4 bytes are used to store the variant index, which is `0` for `White`, `1` for `Gray` and `2` for `RGBA`.

For example, variant `Color::Gray(0x7F)` is encoded as:

```asonb
[
    0x00_00_00_01,              // Variant index for `Gray`, `u32` 1 in this example
    0x00_00_00_7F,              // The value of the variant, in this case it is `u8` 0x7F in raw format
    0x00, 0x00, 0x00, 0x00,     // Padding bytes to fill the size of the enumeration
    0x00, 0x00, 0x00, 0x00,     // ...
    0x00, 0x00, 0x00, 0x00,     // Padding end
]
```

And variant `Color::White` is encoded as:

```asonb
[
    0x00_00_00_00,              // Variant index for `White`, `u32` 0 in this example
    0x00, 0x00, 0x00, 0x00,     // Padding bytes to fill the size of the enumeration
    0x00, 0x00, 0x00, 0x00,     // ...
    0x00, 0x00, 0x00, 0x00,     // ...
    0x00, 0x00, 0x00, 0x00,     // Padding end
]
```

User-defined Enumerations may consume more space than regular Enumerations when the largest variant is much larger than the others.

### 3.5 Individual User-defined Values

Because user-defined values in raw format do not carry type information, they are typically enclosed in user-defined compound values (such as user-defined Objects and Tuples) or type-specified Lists. When used as the document root, they require a dedicated prefix; these are called individual user-defined values.

Format:

```asonb
[
    1-byte type byte (`u8`) `0xE0`-`0xE6`,
    0x00,
    2-byte index of the user-defined value definition (`u16`),
    value data in raw format
]
```

For example, assume the following user-defined Object definition has index `0x00_01`:

```ason
{
    id: i32,
    name: 32-byte length String
}
```

Then an individual Object `{id: 0x42, name: "Alice"}` is encoded as:

```asonb
[
    0xE2,                       // Type byte for user-defined Object
    0x00,                       // Padding
    0x00_01,                    // The index of the user-defined Object definition (0x00_01 in this example)
    [
        0x00_00_00_42,          // The value of field "id" (i32 0x42) in raw format
        "Alice...",             // The value of field "name" (String "Alice") in raw format with null bytes
    ]
]
```

> Individual user-defined values still have a value prefix although their data is in raw format. They are designed to be used as the document root value.

Individual user-defined values can also appear in regular compound values, acting as a bridge between regular values and raw-format user-defined values.

### 3.6 Reference Values

Reference values include Reference String and Reference Byte Data. Reference values are designed to save space in the raw format for String and Byte Data by storing the actual data in a separate section and referencing it by index. The value itself only stores an index, so it is fixed-size (8 bytes) and can be enclosed in user-defined compound values and type-specified Lists.

#### 3.6.1 Reference String

Format:

```asonb
[
    0x53,
    0x00,
    2-byte section index (`u16`),
    4-byte entry index (`u32`),
]
```

Here, `entry index` is the index within a _data index section_, and `section index` identifies the data-section pair.

The data index section format is:

```asonb
[
    0xF3,                                   // Type byte for data index section
    0x00,
    0x00_00,
    4-byte number of index entries (`u32`),
    [                                       // The first index entry
        4-byte item offset (`u32`),
        4-byte item length (`u32`),
    ]
    [                                       // The second index entry
        4-byte item offset (`u32`),
        4-byte item length (`u32`),
    ]
    ...                                     // More index entries
]
```

Where the `item offset` is the offset of the data block in the _data block section_.

The data block section format is:

```asonb
[
    0xF4,                                   // Type byte for data block section
    0x00,
    0x00_00,
    4-byte total length of data blocks (`u32`),
    [                                       // The first data block
        data block,
        null bytes
    ]
    [                                       // The second data block
        data block,
        null bytes
    ]
    ...                                     // More data blocks
]
```

A data block stores either UTF-8 string data or raw bytes:

- When it is String UTF-8 data, it is null-terminated and padded with null bytes to achieve 4-byte alignment for the next data block. The `item length` in the data index section only counts the actual string data, excluding the null bytes.
- When it is data bytes, the `item length` counts the actual data length, and the data block is padded with null bytes to achieve 4-byte alignment for the next data block.

To access a Reference String value, follow these steps:

1. Read the `section index` and `entry index` within the Reference String value prefix.
2. Read the data index section to get the `item offset` and `item length`.
3. Read the data block section to get the actual data.

#### 3.6.2 Reference Byte Data

Format:

```asonb
[
    0x63,
    0x00,
    2-byte section index (`u16`),
    4-byte entry index (`u32`),
]
```

Reference Byte Data is identical to Reference String except for the type byte. Both share the same data index and data block sections.

### 3.7 Discrete Padding Bytes

In some cases, padding bytes (`0x00`) may be inserted between values to achieve alignment. These padding bytes are not part of the actual data and should be ignored when accessing values. These bytes are called _discrete padding bytes_.

Those padding bytes that are appended after value data blocks of some integer values (e.g., `u8`, `u16`), String, and Byte Data to achieve 4-byte alignment for the next value are called _implicit padding bytes_. They are part of the format or their amount is known in advance, so they are not considered discrete padding bytes.

The length of discrete padding bytes is variable (from 1 to any number of bytes) and is determined by the alignment requirements of the values. For example, an ASONB document that only contains a single 64-bit integer value 0x42 would have the following binary representation:

```asonb
[
    0x00, 0x00, 0x00, 0x00          // discrete padding bytes for 8-byte alignment
    [
        0x16, 0x00, 0x00, 0x00,     // Value prefix for 64-bit signed integer
        0x42, 0x00, 0x00, 0x00,     // The low 4 bytes of `i64`
        0x00, 0x00, 0x00, 0x00      // The high 4 bytes of `i64`
    ]
]
```

The 4 discrete padding bytes are inserted before the value to achieve 8-byte alignment for the value data block (i.e., the `[0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]`).

If the `i64` value data block is already 8-byte aligned, discrete padding bytes are not needed.

> Discrete padding bytes must not be inserted between the value prefix and the value data block, and must not be inserted between raw format values.

### 3.8 File Extension and MIME Type

The file extension for ASONB documents is `.asonb`, and the MIME type is `application/asonb`.

## 4 Notes for Implementations

Support for user-defined values and reference values is optional for ASONB access libraries. In most cases, the default ASONB feature set is sufficient.

User-defined values target scenarios where documents must be stored compactly and accessed with high performance.

Reference values target scenarios with large String or Byte Data payloads where minimizing document size is important.

## 5 Comparison with Other Binary Serialization Formats

No binary serialization format is universally best. ASONB draws inspiration from many existing formats and is designed for modern applications and workflows. Its key strengths include a rich data type system, streamability, incremental append/update workflows, and predictable layouts that support memory mapping and random access — all while balancing compactness, performance, and ease of use.

ASONB addresses some of the limitations found in existing formats:

- **BSON**: A document can only represent an Object, offering limited flexibility for rich data types.
- **Protocol Buffers**: Requires schemas and code generation, with complex encoding rules that add friction to simple use cases.
- **Avro**: Relies on schemas and carries significant overhead, making it less suitable for lightweight scenarios.
- **CBOR**: Its data type system is closely tied to JSON, lacking support for richer data types and memory-mapped access.
- **MessagePack**: Offers a straightforward encoding — essentially a compact binary translation of JSON text — but lacks support for random access.

If you are developing a new application and need a binary serialization format for data pipelines or file storage, ASONB is a great choice.

## 6 Linking

- [ASON Specification and library](https://github.com/hemashushu/ason)
- [ASONB Specification and library](https://github.com/hemashushu/asonb)

## 7 License

This project is licensed under the MPL 2.0 License with additional terms. See the files [LICENSE](./LICENSE) and [LICENSE.additional](./LICENSE.additional)
