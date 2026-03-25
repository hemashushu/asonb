# ASONB

_ASONB_ is a well-designed binary representation of ASON, optimized for efficient access and transmission.

ASONB supports incremental updates, streaming and random access, making it suitable for a wide range of use cases, from simple persistence to complex data processing pipelines.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [1 Features](#1-features)
- [2 Specification](#2-specification)
  - [2.1 Encoding](#21-encoding)
    - [2.1.1 Values](#211-values)
    - [2.1.2 Fixed-size Values, User-defined Values, and Raw Format](#212-fixed-size-values-user-defined-values-and-raw-format)
    - [2.1.3 Streaming and Random Access](#213-streaming-and-random-access)
    - [2.1.4 Alignment](#214-alignment)
  - [2.2 Document](#22-document)
    - [2.2.1 Document Header](#221-document-header)
    - [2.2.2 Definitions](#222-definitions)
    - [2.2.3 Data Sections](#223-data-sections)
  - [2.3 Data Types](#23-data-types)
    - [2.3.1 Integers](#231-integers)
    - [2.3.2 Unsigned Integers](#232-unsigned-integers)
    - [2.3.3 Floating Point Numbers](#233-floating-point-numbers)
    - [2.3.4 Boolean Values](#234-boolean-values)
    - [2.3.5 Characters](#235-characters)
    - [2.3.6 DateTime](#236-datetime)
    - [2.3.7 Strings](#237-strings)
    - [2.3.8 Byte Data](#238-byte-data)
    - [2.3.9 Objects](#239-objects)
    - [2.3.10 Tuples](#2310-tuples)
    - [2.3.11 Lists](#2311-lists)
    - [2.3.12 Named Lists](#2312-named-lists)
    - [2.3.13 Enumerations](#2313-enumerations)
  - [2.4 User-defined Values](#24-user-defined-values)
    - [2.4.1 User-defined String](#241-user-defined-string)
    - [2.4.2 User-defined Byte Data](#242-user-defined-byte-data)
    - [2.4.3 User-defined Objects](#243-user-defined-objects)
    - [2.4.4 User-defined Tuples](#244-user-defined-tuples)
    - [2.4.5 User-defined Lists](#245-user-defined-lists)
    - [2.4.6 User-defined Named Lists](#246-user-defined-named-lists)
    - [2.4.7 User-defined Enumerations](#247-user-defined-enumerations)
  - [2.5 Invidiual User-defined Values](#25-invidiual-user-defined-values)
  - [2.6 Reference Values](#26-reference-values)
    - [2.6.1 Reference String](#261-reference-string)
    - [2.6.2 Reference Byte Data](#262-reference-byte-data)
  - [2.7 Discrete Padding Bytes](#27-discrete-padding-bytes)
  - [2.8 File Extension and MIME Type](#28-file-extension-and-mime-type)
- [3 Note for Implementations](#3-note-for-implementations)
- [4 ASONB vs BSON, Protocol Buffers, CBOR, and Other Common Binary Serialization Formats](#4-asonb-vs-bson-protocol-buffers-cbor-and-other-common-binary-serialization-formats)

<!-- /code_chunk_output -->

## 1 Features

- Streamable: ASONB supports forward-only streaming, enabling efficient production and consumption of data in pipeline-oriented workflows.

- Huge data support: ASONB can handle large single values (e.g., long strings, large byte data) and large collections (e.g., lists with millions of items), which is suitable for big data applications, such as AI training data, logs, and databases.

- Efficient access: ASONB stores data in a structured binary format that supports direct memory mapping and random access without intermediate parsing (zero-copy access). This makes it well suited for high-performance storage and retrieval.

- Appendable: ASONB allows incremental updates without rewriting any existing data of the file, making it a practical choice for logs and data tables.

## 2 Specification

### 2.1 Encoding

ASONB document is consisted of ASON _values_ (including primitive values and compound values) in binary format.

#### 2.1.1 Values

Each value is encoded as a _value prefix_ followed by a _value data block_ and optional _end marker_.

```asonb
[
    value prefix (variable length),
    value data block (variable length)
    end marker (optional, for some compound values)
]
```

For example, a 32-bit signed integer value `0x42` is represented as:

```asonb
[
    0x15, 0x00, 0x00, 0x00,     // Value prefix of `i32` type
    0x42, 0x00, 0x00, 0x00,     // The 32-bit signed integer 0x42
]
```

Value prefix includes _base prefix_ and _additional prefix_, where the additional prefix is optional and its content is depends on the value type. The base prefix is 4 bytes long, it consists of a _type byte_, two _extension bytes_ and an _extension modifier byte_:

```asonb
[
    1-byte type (`u8`),
    1-byte extension modifier (`u8`),
    2-bytes extension (`u16`),
]
```

The extension bytes (`u16`) are used for additional information about the value, by default it is not used, and its value is `0x00_00`.

The extension modifier byte (`u8`) is used for change the interpretation of the extension bytes, by default it is also not used, and its value is `0x00`.

The type byte (`u8`) indicates the data type of the value, it is the most important part of the value prefix, it determines how to interpret the value data block.

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
| 0xB2      | Incremental Typed Named List |            |
| 0xC0-0xC3 | Enumeration                  |            |
| 0xC4-0xC7 | Name-predefined Enumeration  |            |

Some frequently used values have their own compact representation leverage the specical type bytes, some of compact representation omit the value data block and some omit the additional prefix. The following table shows compact type bytes and their corresponding values:

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

ASONB also allows user define the layout of values, when using these values in the document root, the following type bytes are used:

| Type Byte | Data Type                           |
|-----------|-------------------------------------|
| 0xE0      | Individual User-defined String      |
| 0xE1      | Individual User-defined Byte Data   |
| 0xE2      | Individual User-defined Object      |
| 0xE3      | Individual User-defined Tuple       |
| 0xE4      | Individual User-defined List        |
| 0xE5      | Individual User-defined Named List  |
| 0xE6      | Individual User-defined Enumeration |

The above three tables show all possible type bytes, a access library should throw an error if it encounters an unrecognized type byte.

The 4 MSB (most significant bit) of the type byte indicates the category of the value:

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

There are some special data in ASONB documents besides values, they also use a byte prefix to indicate their type, this byte is called _special byte_ (it is also `u8`). To distinguish from the type byte, the value of special byte is exclusive from the range of type byte.

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

#### 2.1.2 Fixed-size Values, User-defined Values, and Raw Format

Most primitive values (including Integers, Floats, Booleans, Characters, DateTime, Reference String, and Reference Byte Data) are _fixed-size_, which means the length of their value data block is constant and known in advance. For example, the value data block of `i32` is always 4 bytes long, the value data block of `f64` is always 8 bytes long.

If a fixed-size value is enclosed in a type-specified List, type-specified Named List, user-defined value or individual user-defined value, the value prefix could be omitted, and only the value data block is stored, this is called the _raw format_.

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

And a typed-specified List of `i32` is represented as:

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

As shown above, the raw format is more compact than the regular format, because it omits the value prefix for each item, which is especially beneficial for large amount of values. Moreover, it allows for direct memory mapping and random access without intermediate parsing (a.k.a zero-copy access).

> compact representation can not be used in raw format.

If you want to use the raw format for String, Byte Data, and compound values, you can define the layout, the size and alignment for them in the document definition section, and then use them in raw format, these are called _user-defined values_.

> All user-defined values are fixed-size values, and are encoded in raw format. Note that fixed-size values can only contain fixed-size values, error would occur if a fixed-size value contains a variable-size value.

Since the raw format lacks the type information, a special type byte is used if you want to use a user-defined value as document root value, or to enclose a user-defined value in a regular value, these values are called _individual user-defined values_.

#### 2.1.3 Streaming and Random Access

If data is produced and consumed continuously and is transmitted through the pipe, regular format is used, and values can be only accessed in a streaming manner. Of course, if the ASONB document is small, such as an application configuration object or a message packet, it is possible to load the whole document into memory and access any value directly after deserializing. This is the most common use case for accessing ASONB documents.

By design, ASONB documents can also be used for persisting structured data (such as datatables) in files, the data is usually accessed non-sequentially, high performance random access is required. In this scenario, fixed-size values and raw format are used, and access any value without intermediate parsing or deserialization.

#### 2.1.4 Alignment

All values in ASONB are aligned to 4 bytes (except for the `i64`/`u64`/`f64` numbers), which means that the starting byte of each value (both the value prefix and the value data block) must be a multiple of 4. This allows for efficient memory access and parsing. To achieve this alignment, some padding bytes (`0x00`) may be appended at the end of the value bytes.

For example, a 5-byte String will be followed by 3 null bytes to make the length of the value data block 8 bytes.

For 64-bit numbers (`i64`/`u64`/`f64`), they are usually required to be aligned to 8 bytes, which means that the starting byte of each 64-bit number value data block must be a multiple of 8. To achieve this alignment, some _discrete padding bytes_ (is also `0x00`) may be inserted before the value.

For user-defined values, the required alignment is specified in the definition, the valid alignment ranges from 4 to 512 bytes.

The details of the value layout and the padding rules for each data type are described in the following sections.

> Alignments except for 4-byte alignment are not mandatory if the document is not used for memory mapping and random access.

### 2.2 Document

An ASONB document only allows one value (primitive value or compound value) at the root level, commonly it is an Object or a List, but such as a String or a number is also valid.

In addition to the root value, an ASONB document can have an optional document header, and followed by zero or more definitions and data sections, which are all placed before the root value.

The layout of an ASONB document is:

```asonb
[
    optional document header,
    zero or more definitions,
    zero or more data sections,
    root value
]
```

#### 2.2.1 Document Header

Document header is an 8-byte block of metadata, the content is:

```asonb
[
    0xF0,                           // Type byte for document header
    0x00,                           // Indicates binary file
    0x41, 0x53, 0x4F, 0x4E, 0x42,   // Magic number "ASONB"
    0x01,                           // Features and version
]
```

The 4-bit MSB (most significant bit) of the features and version byte is feature flags, and the remaining 4-bit LSB (least significant bit) is the version number, which is currently `0x01`.

The following table shows all possible feature flags:

| Feature Bit | Description                        |
|-------------|------------------------------------|
| 0b0000      | Default                            |
| 0b0001      | Require user-defined value support |
| 0b0010      | Require reference value support    |

For example, if a document requires user-defined value support and reference value support, the features and version byte should be `0b0011_0001` (0x31).

The access library should throw an error if it encounters a document header with unsupported features or version number.

> The supporting of user-defined value and reference value for ASONB access libraries are not mandatory.

Document header is not mandatory, but it should be included when persisting ASONB documents to files to facilitate format recognition and versioning, or the documents are intended to be used for public APIs, or any features are required.

#### 2.2.2 Definitions

Definitions are used for defining names of Object, names of Enumeration variants, and user-defined values.

The format of name definition is:

```asonb
[
    0xF1,
    0x00,
    1-byte definition type (`u8`),
    0x00,
    4-byte numbers of names (`u32`),
    names (variable length)
]
```

The following table shows all possible definition types for name definition:

| Definition Type | Description               |
|-----------------|---------------------------|
| 0x80            | Object field names        |
| 0xC0            | Enumeration variant names |

The details of the name definitions are described in the "Objects" and "Enumerations" sections.

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

The alignment byte indicates the required alignment for the value data block of the user-defined value, it can be 0 or 2 to 9, which means `2 power alignment` bytes alignment, for example, if the alignment byte is `0x03`, the alignment is `2 power 3 = 8` bytes, thus ASONB allows alignments from 4 to 512 bytes. Alignment other than 0, 2 to 9 is invalid and should be rejected by the access library.

If the alignment byte is `0x00`, it means that the alignment is not specified, and the default alignment (4 bytes) is used. If the ASONB document is not intented to be used for memory mapping and random access, the alignment byte can be set to `0x00` for simplicity.

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

An ASONB document can have max `65536` name definitions and `65536` user-defined value definitions, each definition can be referenced by its index (`0` to `65535`) in values.

The details of the definitions are described in the "User-defined Values" section.

#### 2.2.3 Data Sections

Data sections are used for storing the actual data of the reference value (including Reference String and Reference Byte Data values). Each data section is consisted of a pair of _data index section_ and a _data block section_, an ASONB document can have multiple data section pairs, the layout of data section pairs is:

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

### 2.3 Data Types

The following sections describe the layout of value prefix and value data block for each data type.

#### 2.3.1 Integers

Format:

- `[0x13, 0x00, 0x00, 0x00, 1-byte data, 3-bytes signed extension]`: 8-bit signed integer
- `[0x14, 0x00, 0x00, 0x00, 2-byte data, 2-bytes signed extension]`: 16-bit signed integer
- `[0x15, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit signed integer
- `[0x16, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit signed integer

The type byte indicates the data type of the integer, where the 4-bit LSB indicates the width of the integer number: width = `2 power (type byte & 0x0F)` bits. For example, `5` in `0x15` means `2 power 5 = 32` bits.

The modifier byte and the extension bytes are not used, they are set to `0x00` and `0x00_00`.

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

`i8` and `i16` integers are sign-extended to 32 bits. For example, a 16-bit signed integer `0x8088` is sign extended to `0xFFFF8088`, and it is represented as:

```asonb
[
    0x14,                       // Type byte for `i16`
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_00,                    // Extension bytes, not used in this case
    0x88, 0x80, 0xFF, 0xFF,     // 0xFFFF8088 in little-endian format with sign extension
]
```

For the 64-bit integer, since the value data block is required to be 8-byte aligned, some discrete padding bytes (`0x00`) may be inserted before the value. See the "Discrete Padding Bytes" section for more details.

#### 2.3.2 Unsigned Integers

Format:

- `[0x23, 0x00, 0x00, 0x00, 1-byte data, 0x00, 0x00, 0x00]`: 8-bit unsigned integer
- `[0x24, 0x00, 0x00, 0x00, 2-byte data, 0x00, 0x00]`: 16-bit unsigned integer
- `[0x25, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit unsigned integer
- `[0x26, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit unsigned integer

Unsigned integers are stored in the same way as signed integers, but with a different type byte (`0x23` to `0x26`), and `u8` and `u16` integers are zero-extended to 32 bits.

#### 2.3.3 Floating Point Numbers

Format:

- `[0x35, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit float (IEEE 754)
- `[0x36, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit float (IEEE 754)

When encoding special floating-point values `NaN` (stands for "Not a Number"), the following canonical bit patterns should be used:

- f32 NaN: `0x7FC0_0000` (quiet `NaN`, zero payload)
- f64 NaN: `0x7FF8_0000_0000_0000` (quiet `NaN`, zero payload)

Or use the compact representation for `NaN`:

- `[0xD0, 0x00, 0x00, 0x00]`: f32 `NaN`
- `[0xD1, 0x00, 0x00, 0x00]`: f64 `NaN`

#### 2.3.4 Boolean Values

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

For consistency, it is recommended to use `0` for `false` and `1` for `true`.

A compact representation for Boolean values can also be used:

- `[0xD2, 0x00, 0x00, 0x00]`: `false`
- `[0xD3, 0x00, 0x00, 0x00]`: `true`

#### 2.3.5 Characters

Format:

```asonb
[
    0x41,
    0x00,
    0x00_00,
    4-byte unicode code point (`u32`)
]
```

#### 2.3.6 DateTime

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

- 8-byte timestamp: `i64` (which is split into two 4-byte parts: `timestamp low` and `timestamp high`), it is the number of seconds since the Unix epoch (January 1, 1970), stored in little-endian format. The timestamp can be negative to represent dates before the epoch.
- 4-byte nanoseconds: `u32`, it is the number of nanoseconds since the last second, stored in little-endian format. Nanoseconds is always positive, the final DateTime value is calculated as `timestamp + nanoseconds / 1_000_000_000`.
- 4-byte timezone offset seconds: `i32`, it is the number of seconds offset from UTC, stored in little-endian format. This field is used for representing only. For UTC DateTime values, this field should be set to `0x00_00_00_00`.

#### 2.3.7 Strings

Format:

```asonb
[
    0x50,
    0x00,
    2-byte numbers of padding bytes (`u16`),
    4-byte length of string data (`u32`),
    [
        UTF-8 string data,
        null bytes
    ]
]
```

String data is encoded in UTF-8 format. It is recommended appending one or more null bytes (`0x00`) at the end of the string data to facilitate mapping the string data in memory to C-style null-terminated string, and for 4-byte alignment purposes. The numbers of null bytes is stored in the extension bytes. The length prefix only counts the actual string data, excluding the null bytes.

For example, the string "Hello" is represented as:

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

In the above example, the string data "Hello" is 5 bytes long, so 3 null bytes are appended at the end to make the data block 8 bytes long (to maintain 4-byte alignment).

If the length of string data happens to be multiple of 4, 4-byte null bytes should be also appended (except for empty string), because the string data should be also null-terminated. For example, the string "ABCD" is represented as:

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

##### 2.3.7.1 Chunk String

Chunk String is represented as a sequence of chunks, each chunk is a fragment of the whole String. The last chunk is followed by a 4-byte `0x00` to indicate the end of the string.

Format:

```asonb
[
    0x51,                               // Type byte for Chunk String
    0x00,                               // Extension bytes modifier, not used in this case
    0x00_00,                            // Extension bytes, not used in this case
    [                                   // The first chunk
        4-byte string length (`u32`),
        4-byte numbers of null bytes (`u32`),
        UTF-8 string data,
        null bytes,
    ]
    [                                   // The second chunk
        4-byte string length (`u32`),
        4-byte numbers of null bytes (`u32`),
        UTF-8 string data,
        null bytes,
    ]
    ...,                                // More chunks
    0x00, 0x00, 0x00, 0x00              // End marker for Chunk String
]
```

The Chunk String generator should ensure that each chunk is a valid UTF-8 string fragment, which means that the chunk should not split a Unicode code point into two or more parts.

Each chunk also requires null bytes at the end for both null-termination and alignment, the numbers of null bytes is stored in the chunk header. The length prefix only counts the actual string data in the chunk, excluding the null bytes.

Chunk strings are useful for streaming where the string is produced incrementally.

##### 2.3.7.2 Long String

If the length of a String is greater than `0xFFFF_FFFF` (4GB), the string must be encoded in the "Long String" format:

```asonb
[
    0x52,
    0x00,
    2-byte numbers of null bytes (`u16`),
    4-byte string length low (`u32`),
    4-byte string length high (`u32`),
    [
        UTF-8 string data,
        null bytes
    ]
]
```

##### 2.3.7.3 Empty String

An empty string can be represented as regular string with zero length:

```asonb
[
    0x50,                       // Type byte for String
    0x00,                       // Extension bytes modifier, not used in this case
    0x00_00,                    // Indicates there is no null byte
    0x00, 0x00, 0x00, 0x00,     // String data length (0 byte)
]
```

A compact representation for empty string is provided:

`[0xD4, 0x00, 0x00, 0x00]`: Empty String

#### 2.3.8 Byte Data

Byte data is used for representing a sequence of binary data, such as images, audio, and other type of data that is not provided with ASONB, such as fixed-point numbers, decimal numbers, UUID, etc.

Format:

```asonb
[
    0x60,
    0x00,
    2-byte numbers of padding bytes (`u16`),
    4-byte length of byte data (`u32`),
    [
        data bytes,
        padding bytes
    ]
]
```

For example, a Byte Data of `0xDE, 0xAD, 0xBE, 0xEF, 0x00` (5 bytes) is represented as:

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

##### 2.3.8.1 Chunk Byte Data

Chunk Byte Data is represented as a sequence of chunks, each chunk is a fragment of the whole Byte Data. The last chunk is followed by a 4-byte `0x00` to indicate the end of the Byte Data.

```asonb
[
    0x61,                               // Data type for Chunk Byte Data
    0x00,                               // Extension bytes modifier, not used in this case
    0x00_00,                            // Extension bytes, not used for in this case
    [                                   // The first chunk
        4-byte data length,
        4-byte numbers of padding bytes,
        data,
        padding bytes,
    ]
    [                                   // The second chunk
        4-byte data length,
        4-byte numbers of padding bytes,
        data,
        padding bytes,
    ]
    ... ,                               // More chunks
    0x00, 0x00, 0x00, 0x00              // End marker for Chunk Byte Data
]
```

##### 2.3.8.2 Long Byte Data

If the length of Byte Data is greater than `0xFFFF_FFFF` (4GB), the Byte Data must be encoded in the "Long Byte Data" format:

```asonb
[
    0x62,
    0x00,
    2-byte numbers of padding bytes (`u16`),
    4-byte byte data length low (`u32`),
    4-byte byte data length high (`u32`),
    [
        data bytes,
        padding bytes
    ]
]
```

##### 2.3.8.3 Empty Byte Data

An empty Byte Data can be represented as regular Byte Data with zero length:

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

#### 2.3.9 Objects

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

Key-value pairs are a sequence of key-value pair without any separator, and it is followed by an end marker (`0xFF, 0xFF, 0xFF, 0xFF`) to indicate the end of the Object.

Key-value pair format:

```asonb
[
    key,
    value
]
```

The _key_ is an identifier in ASON, but it is represented as a regular string in ASONB for simplicity. The _value_ can be of any type.

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

##### 2.3.9.1 Name-predefined Object

To save space, field names can be defined in the name definition, and then can be referenced by their indexes in the Object value.

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

In order to use the name-predefined Object, the field names must be defined in the name definition section. The field name definition has the following format:

```asonb
[
    0xF1,
    0x00,
    0x80,
    0x00,
    4-byte numbers of field names (`u32`),
    field names (variable length)
]
```

The field names are just a sequence of regular String values without any separator.

Consider there is an Object with this structure:

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
    0x00_00_00_02,              // Numbers of field names, `u32` 2 in this example
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

Note that field name definition only contains the names, the type of values is not included, thus the values are still encoded in regular format with value prefixes.

#### 2.3.10 Tuples

Tuples are collections of values, each value can be of any type.

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

Values are a sequence of values without any separator, and it is followed by an end marker (`0xFF, 0xFF, 0xFF, 0xFF`) to indicate the end of the Tuple.

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

#### 2.3.11 Lists

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

Items are just a sequence of values without any separator, and it is followed by an end marker (`0xFF, 0xFF, 0xFF, 0xFF`) to indicate the end of the List.

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

##### 2.3.11.1 Type-specified Lists

Type-specified Lists are Lists with a specified data type for items, values are encoded in raw format, they eliminate the need for value prefixes for each item, thus more compact than regular Lists.

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

##### 2.3.11.2 Incremental Type-specified Lists

Incremental type-specified Lists are type-specified Lists without the list length field, the end of the list is indicated by the end of document (or EOF). Incremental type-specified Lists are only allowed to be used as the root value of an ASONB document.

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

#### 2.3.12 Named Lists

Named Lists are similar to Lists, but each value is associated with a name. The name is usually a string or number, but it can be of any type values.

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

The name-value pairs are just a sequence of name-value pairs without any separator, and it is followed by an end marker (`0xFF, 0xFF, 0xFF, 0xFF`) to indicate the end of the Named List.

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

##### 2.3.12.1 Type-specified Named Lists

Type-specified Named Lists are Named Lists with a specified data type for keys and values, names and values are both encoded in raw format.

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

##### 2.3.12.2 Incremental Type-specified Named Lists

Incremental type-specified Named Lists are type-specified Named Lists without the list length field, the end of the list is indicated by the end of document (or EOF). Incremental type-specified Named Lists are only allowed to be used as the root value of an ASONB document.

Format:

```asonb
[
    0xB2,
    1-byte value data type modifier (`u8`),
    2-byte value data type (`u16`),
    0x00,
    1-byte key data type modifier (`u8`),
    2-byte key data type (`u16`),
    name-value pairs,
    EOF
]
```

#### 2.3.13 Enumerations

Enumerations represent a custom data type that consists of a type name and a set of named variants. A variant can have associated data, which can be of any type.

There are four kinds of variants in ASON:

- Variant without value (e.g. `Option::None`)
- Variant with single value (e.g. `Option::Some(123)`)
- Object-like variant (e.g. `Shape::Rectangle { width: 10, height: 20 }`)
- Tuple-like variant (e.g. `Color::RGB(255_u8, 127_u8, 63_u8)`)

##### 2.3.13.1 Variant without value

Variant without value is the simplest kind of variant, it only contains the type name and the variant name.

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

Type name and variant name are identifiers in ASON, but they are represented as regular strings in ASONB for simplicity.

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

##### 2.3.13.2 Variant with single value

Variant with single value can carry a single value of any type.

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

##### 2.3.13.3 `Option` and `Result` Variants

For the common `Option` and `Result` enumerations, there are compact representations for their variants:

- `Option::None`: `[0xD6, 0x00, 0x00, 0x00]`
- `Option::Some(value)`: `[0xD7, 0x00, 0x00, 0x00, value]`
- `Result::Ok(value)`: `[0xD8, 0x00, 0x00, 0x00, value]`
- `Result::Err(value)`: `[0xD9, 0x00, 0x00, 0x00, value]`

##### 2.3.13.4 Object-like variant

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

Consider the this enumeration:

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

You may notice that the Object-like variant is very similar to the variant with single Object value, the only difference is the data type byte (`0xC2` for Object-like variant and `0xC1` for variant with single value), the encoding format of the Object value is the same in both cases. This is designed intentionally.

##### 2.3.13.5 Tuple-like variant

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

The binary representation of tuple-like variant is similar to the variant with single Tuple value, the only difference is the data type byte (`0xC3` for tuple-like variant and `0xC1` for variant with single value).

##### 2.3.13.6 Name-predefined Variants

To save space, variant names can be defined in the name definition, and then can be referenced by their indexes in the variant value.

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

In order to use the name-predefined variant, the variant names must be defined in the name definition section. The variant name definition has the following format:

```asonb
[
    0xF1,
    0x00,
    0xC0,
    0x00,
    4-byte numbers of variant names (`u32`),
    type name,
    variant names (variable length)
]
```

The variant names are just a sequence of regular String values without any separator.

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
    0x00_00_00_02,                      // Numbers of variant names, `u32` 2 in this example
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

Name-predefined variants eliminate the type name and variant name from the variant value, thus more compact than regular variants.

### 2.4 User-defined Values

You can specify the layout (such as the length, alignment, fields, data type etc.) of a value in the definition section, and then reference the definition in the value, thus the value can be encoded in raw format without value prefix, which is more compact than regular values.

The support of user-defined values is optional for ASONB access libraries, if a document requires user-defined values, the document header should be present and the feature flag `0b0001` should be set.

The following sections describe the layout of user-defined values definitions.

#### 2.4.1 User-defined String

User-defined String has a fixed capacity (i.e., the max length), and the actual string data is padded with null bytes to fill the capacity, the capacity is defined in the definition.

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

Assuming the above definition is at index `0x00_01`, then the value of this user-defined String can be encoded in raw format without value prefix. For example, the following demonstrates a type-specified List of this user-defined String:

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

- The capacity should be multiple of 4.
- The definition only defines the capacity of the string, it does not define the actual length of the string data. The actual length can be determined by finding the first null byte in the string data, or by using another value to store the actual length.

#### 2.4.2 User-defined Byte Data

User-defined Byte Data has a fixed capacity, and the actual data is padded to fill the capacity, the capacity is defined in the definition.

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

- The capacity should be multiple of 4.
- The definition only defines the capacity of the Byte Data, it does not define the actual length of the Byte Data.

#### 2.4.3 User-defined Objects

If an Object has a fixed layout (i.e., the field names, the data type and size of values are all specified), all its fields can be encoded in raw format without field names and value prefixes.

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

The 1-byte alignment indicates the required alignment for the data block of the Object, the value is equal to the maximum alignment of all fields. For example, if an Object has two fields, one is `i32` (4-byte alignment) and the other is `f64` (8-byte alignment), then the alignment byte should be set to `0x03` for 8-byte alignment.

The field definitions are a sequence of field definition without any separator, each field definition has the following format:

```asonb
[
    0x00,
    1-byte data type modifier (`u8`),
    2-byte data type (`u16`),
    field name
]
```

Where field name is a regular String value.

For example, consider an Object with this layout:

```ason
{
    id: i32
    name: 32-byte length String
}
```

Asuming we have already defined a user-defined String with a capacity of 32 bytes, and its definition index is `0x00_01`, then we can define the user-defined Object as:

```asonb
[
    0xF2,                       // Type byte for user-defined value definition
    0x00,                       // Padding
    0x80,                       // Definition type for user-defined Object (0x80)
    0x00,                       // Alignment, 0 for default alignment (4 bytes)
    0x00_00_00_02,              // Numbers of field definitions, `u32` 2 in this example
    [                           // The first field
        0x00,                   // Padding
        0x00,                   // Data type modifier, set to `0x00_00` for fixed-size primitive types
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

Internal field padding bytes may be added between fields to satisfy the alignment requirement, it is different from the padding rule of C structs, which rearranges the fields to minimize the packing size, the order of fields in user-defined Objects is fixed and cannot be rearranged.

> User-defined Objects in raw format are significantly more compact than the regular Object values.

#### 2.4.4 User-defined Tuples

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

The element data types are a sequence of data type without any separator, each data type is consists of 4 bytes:

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

Asuming we have already defined a user-defined String with a capacity of 32 bytes, and its definition index is `0x00_01`, then we can define the user-defined Tuple as:

```asonb
[
    0xF2,                       // Type byte for user-defined value definition
    0x00,                       // Padding
    0x90,                       // Definition type for user-defined Tuple (0x90)
    0x00,                       // Alignment, 0 for default alignment (4 bytes)
    0x00_00_00_03,              // Numbers of elements, `u32` 3 in this example
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

#### 2.4.5 User-defined Lists

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

#### 2.4.6 User-defined Named Lists

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

#### 2.4.7 User-defined Enumerations

A user-defined Enumeration is an Enumeration with predefined variants.

Format:

```asonb
[
    0xF2,
    0x00,
    0xC0,
    1-byte alignment (`u8`),
    4-byte numbers of variants (`u32`),
    type name,
    variant definitions
]
```

Variant definitions are a sequence of variant definition without any separator, each variant definition has the following format:

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

Note that the size of a user-defined Enumeration value is fiexed, and it is determined by the maximum size of its variants and an extra `u32` which is used to store the variant index. For example, consider the following Enumeration:

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

Note that user-defined Enumerations may waste more space than regular Enumerations if the size of the largest variant is much larger than the other variants.

### 2.5 Invidiual User-defined Values

Since user-defined values in raw format lack the type information, they are usually enclosed in other user-defined compound values (such as user-defined Object, user-defined Tuple, etc.) and type-specified Lists, if they are used as the root value of the document, a specific prefix is required to indicate the type, these values are called individual user-defined values.

Format:

```asonb
[
    1-byte type byte (`u8`) `0xE0`-`0xE6`,
    0x00,
    2-byte index of the user-defined value definition (`u16`),
    value data in raw format
]
```

For example, assume the below user-defined Object definition has index `0x00_01`:

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

> Individual user-defined values still have value prefix although their data is in raw format, they are designed to be used as the document root value.

Individual user-defined values can also be used in regular compound values, in this case, individual user-defined values are the bridge between regular values and user-defined values in raw format.

### 2.6 Reference Values

Reference values including Reference String and Reference Byte Data. Reference values are designed for saving raw format space of String and Byte Data by storing the actual data in a separate section and referencing it by index, the value itself only stores an index, so it is fixed-size (8 bytes) and can be enclosed in user-defined compound values and type-specified Lists.

#### 2.6.1 Reference String

Format:

```asonb
[
    0x53,
    0x00,
    2-byte section index (`u16`),
    4-byte entry index (`u32`),
]
```

Where the index is the index of the _data index section_ entry, and the `section index` is the index of data section pairs.

The data index section format is:

```asonb
[
    0xF3,                                   // Type byte for data index section
    0x00,
    0x00_00,
    4-byte numbers of index entries (`u32`),
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

data block storing String UTF-8 data or data bytes:

- When it is String UTF-8 data, it is null-terminated and padded with null bytes to achieve 4-byte alignment for the next data block, the `item length` in the data index section only counts the actual string data, excluding the null bytes.
- When it is data bytes, the `item length` counts the actual data length, and the data block is padded with null bytes to achieve 4-byte alignment for the next data block.

In order to access a Reference String value, you need to follow these steps:

1. Read the `section index` and `entry index` within the Reference String value prefix.
2. Read the data index section to get the `item offset` and `item length`.
3. Read the data block section to get the actual data.

#### 2.6.2 Reference Byte Data

Format:

```asonb
[
    0x63,
    0x00,
    2-byte section index (`u16`),
    4-byte entry index (`u32`),
]
```

Reference Byte Data is similar to Reference String except that the type byte is different, and they share the same data index section and data block section.

### 2.7 Discrete Padding Bytes

In some cases, padding bytes (`0x00`) may be inserted between values to achieve alignment. These padding bytes are not part of the actual data, and they should be ignored when accessing the values. These bytes are called _discrete padding bytes_.

Those padding bytes which are appended after value data blocks of some integer values (e.g., `u8`, `u16`), String, and Byte Data to achieve 4-byte alignment for the next value, are called _implicit padding bytes_, they are part of the format or the amount is known in advance, thus they are not considered as discrete padding bytes.

The length of discrete padding bytes is variable (from 1 to any number of bytes), and it is determined by the alignment requirements of the values. For example, an ASONB document that only contains a single 64-bit integer value 0x42, the binary representation would be:

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

The 4 discrete padding bytes are preceded before the value to achieve 8-byte alignment for the value data block (i.e., the `[0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]`).

In some cases, the `i64` value data block happens to be 8-byte aligned, then the discrete padding bytes are not needed.

> The discrete padding bytes are not allowed to be inserted between the value prefix and the value data, and not allowed to be inserted between raw format values.

### 2.8 File Extension and MIME Type

The file extension for ASONB documents is `.asonb`, and the MIME type is `application/asonb`.

## 3 Note for Implementations

The support of user-defined values and reference values is optional for ASONB access libraries, in general, the default ASONB features are sufficient for common use cases.

User-defined values are designed for specific use cases where the document is required to be persisted to file in a compact format, and the access performance is critical.

Reference values are designed for specific use cases where the document contains large String or Byte Data, and the document capacity needs to be as small as possible.

## 4 ASONB vs BSON, Protocol Buffers, CBOR, and Other Common Binary Serialization Formats

TODO

<!--
Reference links

- BSON: https://bsonspec.org/
- Protocol Buffers: https://protobuf.dev/
- MessagePack: https://msgpack.org/
- CBOR: https://cbor.io/
- Avro: https://avro.apache.org/
-->