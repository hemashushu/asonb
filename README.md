# ASONB

_ASONB_ is a well-designed binary representation of ASON, optimized for efficient access and transmission.

ASONB supports incremental updates, streaming, and random access, making it suitable for a wide range of use cases, from simple persistence to complex data processing pipelines.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [1 Features](#1-features)
- [2 Specification](#2-specification)
  - [2.1 Layout](#21-layout)
    - [2.1.1 Value Layout](#211-value-layout)
    - [2.1.2 Random Access](#212-random-access)
    - [2.1.3 Alignment](#213-alignment)
    - [2.1.4 Document Layout](#214-document-layout)
  - [2.2 Header](#22-header)
    - [2.2.1 Document Header](#221-document-header)
    - [2.2.2 Definitions Header](#222-definitions-header)
  - [2.3 Primitive Data Types](#23-primitive-data-types)
    - [2.3.1 Integers](#231-integers)
    - [2.3.2 Unsigned Integers](#232-unsigned-integers)
    - [2.3.3 Floating Point Numbers](#233-floating-point-numbers)
    - [2.3.4 Boolean Values](#234-boolean-values)
    - [2.3.5 Characters](#235-characters)
    - [2.3.6 DateTime](#236-datetime)
    - [2.3.7 Strings](#237-strings)
    - [2.3.8 Byte Data](#238-byte-data)
  - [2.5 Objects](#25-objects)
    - [2.5.1 Field Names Predefined Object](#251-field-names-predefined-object)
    - [2.5.2 Fixed-size Objects](#252-fixed-size-objects)
  - [2.6 Tuples](#26-tuples)
    - [2.6.1 Variable-length tuples](#261-variable-length-tuples)
  - [2.9 Lists](#29-lists)
    - [2.9.1 Variable-length lists](#291-variable-length-lists)
  - [2.7 Named Lists](#27-named-lists)
    - [2.7.1 Variable-length named lists](#271-variable-length-named-lists)
  - [2.8 Enumerations](#28-enumerations)
    - [2.8.1 Variant without value](#281-variant-without-value)
    - [2.8.2 Variant with single value](#282-variant-with-single-value)
    - [2.8.3 Object-like variant](#283-object-like-variant)
    - [2.8.4 Tuple-like variant](#284-tuple-like-variant)
  - [2.9 Discrete Padding Bytes](#29-discrete-padding-bytes)
- [3 ASONB vs BSON, Protocol Buffers, CBOR, and other binary serialization formats](#3-asonb-vs-bson-protocol-buffers-cbor-and-other-binary-serialization-formats)

<!-- /code_chunk_output -->

## 1 Features

- Efficient access: ASONB stores data in a structured binary format that supports direct memory mapping and random access without intermediate parsing (zero-copy access). This makes it well suited for high-performance storage and retrieval.

- Huge data support: ASONB can handle large single values (e.g., long strings, large byte data) and large collections (e.g., lists with millions of items), which is suitable for big data applications, such as AI training data, logs, and databases.

- Streamable: ASONB supports forward-only streaming, enabling efficient production and consumption of data in pipeline-oriented workflows.

- Appendable: ASONB allows incremental updates without rewriting any existing data of the file, making it a practical choice for logs and data tables.

## 2 Specification

### 2.1 Layout

An ASONB document is consisted of ASON values in binary format.

#### 2.1.1 Value Layout

Each value is encoded as a 4-byte value prefix followed by a sequence of bytes representing the value:

```asonb
[
    value prefix (4 bytes),
    value bytes (variable length)
]
```

The value prefix consists of a type byte, a flag byte and two extension bytes:

```asonb
[
    1-byte type,                // Indicates the data type of the value
    1-byte extension modifier,  // Depending on the type, defaults to 0x00
    2-bytes extension,          // Depending on the type, defaults to 0x00_00
]
```

The 4 MSB (most significant bit) of the type byte indicates the category of the value:

| 4 MSB    | Category                     |
|----------|------------------------------|
| 0x0      | Reserved                     |
| 0x1      | Signed Integer               |
| 0x2      | Unsigned Integer             |
| 0x3      | Floating Point Number        |
| 0x4      | Boolean, Character, DateTime |
| 0x5      | String, Byte Data            |
| 0x6-0x7  | Reserved                     |
| 0x8      | Object                       |
| 0x9      | Tuple                        |
| 0xA      | List                         |
| 0xB      | Named List                   |
| 0xC      | Enumeration                  |
| 0xD-0xE  | Reserved                     |
| 0xF      | Headers                      |

The following table shows the type byte for primitive data types:

| Type Byte | Data Type             |
|-----------|-----------------------|
| 0x13      | i8                    |
| 0x14      | i16                   |
| 0x15      | i32                   |
| 0x16      | i64                   |
| 0x23      | u8                    |
| 0x24      | u16                   |
| 0x25      | u32                   |
| 0x26      | u64                   |
| 0x30      | f32 `NaN`             |
| 0x31      | f64 `NaN`             |
| 0x35      | f32                   |
| 0x36      | f64                   |
| 0x40      | Boolean `false`       |
| 0x41      | Boolean `true`        |
| 0x42      | Character             |
| 0x43      | DateTime              |
| 0x50      | String                |
| 0x51      | Chunk String          |
| 0x52      | Long String           |
| 0x54      | Fixed-size String     |
| 0x57      | Empty String          |
| 0x58      | Byte Data             |
| 0x59      | Chunk Byte Data       |
| 0x5A      | Long Byte Data        |
| 0x5C      | Fixed-size Byte Data  |
| 0x5F      | Empty Byte Data       |

The following table shows the type byte for compound data types:

| Type Byte | Data Type                             |
|-----------|---------------------------------------|
| 0x80      | Object                                |
| 0x81      | Field names predefined Object         |
| 0x82      | Fixed-size Object                     |
| 0x90      | Tuple                                 |
| 0x92      | Fixed-size Tuple                      |
| 0xA0      | List                                  |
| 0xA1      | Type-specified List                   |
| 0xA2      | Fixed-size List                       |
| 0xB0      | Named List                            |
| 0xB1      | Fixed-size Named List                 |
| 0xC0-0xC3 | Enumeration                           |
| 0xC4-0xC7 | Variant names predefined Enumeration  |
| 0xC8-0xCB | Fixed-size Enumeration                |
| 0xCC      | `Option::None` variant                |
| 0xCD      | `Option::Some(...)` variant           |

The following table shows the type byte for other data than values:

| Type Byte | Data Type                           |
|-----------|-------------------------------------|
| 0xF0      | Document Header                     |
| 0xF1      | Definitions Header                  |
| 0x00      | Padding byte for alignment          |
| 0xFF      | End marker for some compound values |

The extension bytes are used for additional information about the value, such as the number of tailing padding bytes in String and Byte Data, the data type in the type-specified List. The default value is `0x00_00`.

The extension modifier byte is used for change the interpretation of the extension bytes, the default value is `0x00`. When it is set to `0x01`, the extension bytes are interpreted as the index of the definition header for type-specified values (such as fixed-size Object, fixed-size Tuple, and type-specified List).

#### 2.1.2 Fixed-size Values and Raw Format

If a value is fixed-size and enclosed in type-specified values, the value prefix would be omitted, and only the value bytes are stored, this is called "raw format".

For example, a regular List of `i32` is represented as:

```asonb
[
    0xA0,                               // Type byte for List
    0x00,                               // Extension bytes modifier, not used for regular List
    0x00_00,                            // Extension bytes, not used for regular List
    [0x00_00_00_15, 0x00_00_00_11],     // The first item `i32` 0x11 with prefix
    [0x00_00_00_15, 0x00_00_00_13],     // The second item `i32` 0x13 with prefix
    [0x00_00_00_15, 0x00_00_00_17],     // The third item `i32` 0x17 with prefix
    ...                                 // More items
    0xFF, 0xFF, 0xFF, 0xFF,             // List end marker
]
```

> Note: Brackets in the binary data examples in this document are for readability only, they are neither part of the actual binary data, nor do they indicate the hierarchy of the data. The actual binary data is a flat sequence of bytes.

A typed-specified List of `i32` is represented as:

```asonb
[
    0xA1,                       // Type byte for type-specified List
    0x00,                       // Extension bytes modifier, not used for primitive data types
    0x00_15,                    // Data type of items, 0x15 means `i32`
    [
        0x00_00_00_11,          // The first item `i32` 0x11 in raw format
        0x00_00_00_13,          // The second item `i32` 0x13 in raw format
        0x00_00_00_17,          // The third item `i32` 0x17 in raw format
        ...                     // More items
    ]
    0xFF, 0xFF, 0xFF, 0xFF,     // List end marker
]
```

#### 2.1.2 Streaming and Random Access

There are two major use scenarios for ASONB documents:

##### 2.1.2.1 Transmittion

Transmit data in the pipeline, the data is usually produced and consumed in a forward-only manner, in this scenario, regular format is used, values can be only accessed in a streaming manner. Streaming access is more efficient especially for long time tasks which produce data continuously.

Of cause, for the small size ASONB document, it is possible to load the whole document into memory and access any value directly after parsing and deserializing.

##### 2.1.2.2 Persistence

Persist data in files, the data is usually accessed non-sequentially, high performance random access is required. In this scenario, the size of values is fixed, the memory layout of values is predefined, thus raw format is usually used.

Most primitive values (such as Integers, Floats, Booleans, Characters, and DateTime) are fixed-size values, they can be enclosed in type-specified values in raw format, other values (such as Strings, Byte Data, Objects, Tuples, Lists) are variable size, their size or layout must be defined first and referenced through definition index if you want to encode them in raw format.

Note that fixed-size values can only contain fixed-size values, for example, a fixed-size Object can only contain fixed-size String, error will occur if it contains regular String.

#### 2.1.3 Alignment

All values in ASONB are aligned to 4 bytes (except for the `i64`/`u64`/`f64` numbers), which means that the starting byte of each value (both the value prefix and the value data) must be a multiple of 4. This allows for efficient memory access and parsing. To achieve this alignment, some padding bytes (0x00) may be inserted before or at the end of the value bytes.

For example:

- A 1-byte `i8` number will be followed by 3 padding bytes to make the length of the value data block 4 bytes.
- A 5-byte string will be followed by 3 padding bytes to make the length of the value data block 8 bytes.

For the 64-bit numbers (`i64`/`u64`/`f64`), they are required to be aligned to 8 bytes, which means that the starting byte of each 64-bit number value data must be a multiple of 8. To achieve this alignment, some discrete padding bytes (0x00) may be inserted before the value.

The details of the value layout and the padding rules for each data type are described in the following sections.

#### 2.1.4 Document Layout

ASONB document only allows a single value (including compound values) at the root level.

The following show an simplest ASONB document containing a single integer value `0x42`:

```asonb
[
    0x15, 0x00, 0x00, 0x00,     // Value prefix of `i32` type
    0x42, 0x00, 0x00, 0x00,     // The 32-bit signed integer 0x42
]
```

### 2.2 Header

#### 2.2.1 Document Header

An ASONB document may optionally start with a header, which is a 8-byte block of metadata:

```asonb
[
    0xF0,                           // Type byte for document header
    0x00,                           // Padding byte
    0x41, 0x53, 0x4F, 0x4E, 0x42,   // Magic number "ASONB"
    0x01,                           // Version number
]
```

Header is not mandatory, but it is recommended to include it when persisting ASONB documents to files to facilitate format recognition and versioning.

#### 2.2.2 Definitions Header

Any number of definition headers can be included between the document header and the root value.

Format:

```asonb
[
    0xF1,                           // Type byte for definitions header
    0x00,                           // Extension bytes modifier
    2-byte definition type (`u16`), // Indicates the type of definitions
    data                            // Definition data, the format depends on the definition type
]
```

Definition headers are used for defining fixed-size values and the object and variant names. The definition type is a 2-byte unsigned integer:

| Definition Type | Description               |
|-----------------|---------------------------|
| 0x00_50         | Fixed-size String         |
| 0x00_58         | Fixed-size Byte Data      |
| 0x00_80         | Fixed-size Object         |
| 0x00_90         | Fixed-size Tuple          |
| 0x00_A0         | Fixed-size List           |
| 0x00_B0         | Fixed-size Named List     |
| 0x00_C0         | Fixed-size Enumeration    |
| 0x01_80         | Object field names        |
| 0x01_C0         | Enumeration variant names |

An ASONB document can have max 0xFFFF (65535) definition headers, each definition can be referenced by its index (0 to 65534) in values.

The details of the definition headers and the definition data format are described in the following sections.

### 2.3 Primitive Data Types

#### 2.3.1 Integers

Format:

- `[0x13, 0x00, 0x00, 0x00, 1-byte data, 0x00, 0x00, 0x00]`: 8-bit signed integer
- `[0x14, 0x00, 0x00, 0x00, 2-byte data, 0x00, 0x00]`: 16-bit signed integer
- `[0x15, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit signed integer
- `[0x16, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit signed integer

The first 4 bytes are the value prefix, the type byte indicates the data type of the integer, and the extension bytes are not used, they are set to `0x00_00`.

The 4-bit LSB of the type byte indicates the width of the integer number, for example, `5` in `0x15` means `2 power 5 = 32` bits.

The integer data is stored in little-endian format.

Example of a 32-bit signed integer `0x42`:

```asonb
[
    0x15, 0x00, 0x00, 0x00,     // Prefix for `i32` type
    0x42, 0x00, 0x00, 0x00,     // `i32` 0x42 in little-endian format
]
```

`i8` and `i16` integers are not sign-extended, they are stored in the least significant bytes of the value block, and the remaining bytes are filled with `0x00` for padding. For example, a 16-bit signed integer `0x1234` is represented as:

```asonb
[
    0x14, 0x00, 0x00, 0x00,     // Prefix for `i16` type
    0x34, 0x12,                 // `i16` 0x1234 in little-endian format
    0x00, 0x00                  // Extension bytes modifiers
]
```

For the 64-bit signed integer, it is required to be 8-byte aligned, some discrete padding bytes (0x00) may be inserted before the value bytes. See the "Discrete Padding Bytes" section for more details.

#### 2.3.2 Unsigned Integers

Unsigned integers are stored in the same way as signed integers, but with a different type byte (0x23 to 0x26).

Format:

- `[0x23, 0x00, 0x00, 0x00, 1-byte data, 0x00, 0x00, 0x00]`: 8-bit unsigned integer
- `[0x24, 0x00, 0x00, 0x00, 2-byte data, 0x00, 0x00]`: 16-bit unsigned integer
- `[0x25, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit unsigned integer
- `[0x26, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit unsigned integer

#### 2.3.3 Floating Point Numbers

Format:

- `[0x35, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit float (IEEE 754)
- `[0x36, 0x00, 0x00, 0x00, 8-byte data]`: 64-bit float (IEEE 754)

When encoding special floating-point values NaN (which stands for "Not a Number"), the following canonical bit patterns should be used:

- f32 NaN: 0x7FC0_0000  (quiet NaN, zero payload)
- f64 NaN: 0x7FF8_0000_0000_0000  (quiet NaN, zero payload)

Or use the type byte for NaN:

- `[0x30, 0x00, 0x00, 0x00]`: f32 NaN
- `[0x31, 0x00, 0x00, 0x00]`: f64 NaN

#### 2.3.4 Boolean Values

Format:

- `[0x40, 0x00, 0x00, 0x00]`: false
- `[0x41, 0x00, 0x00, 0x00]`: true

#### 2.3.5 Characters

Format:

`[0x42, 0x00, 0x00, 0x00, 4-byte data]`: 32-bit unicode code point (`u32`)

#### 2.3.6 DateTime

Format:

```asonb
[
    0x43, 0x00, 0x00, 0x00,
    4-byte timestamp_low,
    4-byte timestamp_high,
    4-byte nanoseconds,
    4-byte timezone_offset_seconds
]
```

Where:

- 8-byte timestamp: i64 (which is split into two 4-byte parts: `timestamp_low` and `timestamp_high`), it is the number of seconds since the Unix epoch (January 1, 1970), stored in little-endian format. The timestamp can be negative to represent dates before the epoch.
- 4-byte nanoseconds: u32, it is the number of nanoseconds since the last second, stored in little-endian format. The final DateTime value is calculated as `timestamp + nanoseconds / 1_000_000_000`.
- 4-byte timezone_offset_seconds: i32, it is the number of seconds offset from UTC, stored in little-endian format. This field is used for representing only. For UTC DateTime values, this field should be set to `0`.

#### 2.3.7 Strings

Format:

```asonb
[
    0x50,
    0x00,
    2-byte numbers of padding bytes (`u16`),
    4-byte length of string data (`u32`),
    UTF-8 string data,
    null bytes
]
```

String data is encoded in UTF-8 format. It is recommended appending one or more null bytes (`0x00`) at the end of the string data to facilitate mapping the string data in memory to C-style null-terminated string without copying, additionally, for 4-byte alignment purposes. The numbers of null bytes is stored in the extension bytes of the value prefix. The length prefix only counts the actual string data, excluding the null bytes.

For example, the string "Hello" is represented as:

```asonb
[
    0x50,                           // Type byte for String
    0x00,                           // Extension bytes modifier
    0x00_03,                        // Indicates there are 3 null bytes
    0x05, 0x00, 0x00, 0x00,         // String data length (5 bytes)
    0x48, 0x65, 0x6C, 0x6C, 0x6F,   // "Hello" in UTF-8
    0x00, 0x00, 0x00                // Null bytes for both null-termination and alignment
]
```

In the above example, the string data "Hello" is 5 bytes long, so 3 null bytes are required at the end to make the data block 8 bytes long (to maintain 4-byte alignment).

If the length of a string happens to be multiple of 4, 4-byte null bytes should be appended yet, because the string data also should be null-terminated. For example, the string "ABCD" is represented as:

```asonb
[
    0x50,                           // Type byte for String
    0x00,                           // Extension bytes modifier
    0x00_04,                        // Indicates there are 4 null bytes
    0x04, 0x00, 0x00, 0x00,         // String data length (4 bytes)
    0x41, 0x42, 0x43, 0x44,         // "ABCD" in UTF-8
    0x00, 0x00, 0x00, 0x00          // Null bytes for both null-termination and alignment
]
```

Note that when calculating the length of the whole string value, it should cover:

- the value prefix (4 bytes),
- the length prefix (4 bytes),
- the string data
- the null bytes.

For example, the total length of the string "Hello" is:

4 (value prefix) + 4 (length prefix) + 5 (string data) + 3 (null bytes) = 16 bytes.

##### 2.3.7.1 Chunk String

Chunk String is represented as a sequence of chunks, each chunk has a length prefix, a number of null bytes, the string data and the null bytes. The last chunk is followed by a 4-byte `0x00` to indicate the end of the string.

Format:

```asonb
[
    0x51,                               // Type byte for chunk string
    0x00,                               // Extension bytes modifier
    0x00, 0x00,                         // Extension bytes, not used for chunk string
    [                                   // The first chunk
        4-byte string length,
        4-byte numbers of null bytes,
        UTF-8 string data,
        null bytes,
    ]
    [                                   // The second chunk
        4-byte string length,
        4-byte numbers of null bytes,
        UTF-8 string data,
        null bytes,
    ]
    ...,                                // More chunks
    0x00, 0x00, 0x00, 0x00              // End marker for chunk string
]
```

Chunk strings are useful for streaming scenarios where the string is produced incrementally.

##### 2.3.7.2 Long String

If the length of a string is greater than 0xFFFF_FFFF (4GB), the string must be encoded in another format:

```asonb
[
    0x52,                               // Type byte for long string
    0x00,                               // Extension bytes modifier
    2-byte numbers of null bytes,
    4-byte string length low,           // Low part of the `u64` length
    4-byte string length high,          // High part of the `u64` length
    UTF-8 string data,
    null bytes
]
```

##### 2.3.7.3 Fixed-size String

Fixed-size string has a fixed capacity (the max length), and the actual string data is padded with null bytes to fill the capacity, the capacity is defined in the definition header.

The Fixed-size string definition header has the following format:

```asonb
[
    0xF1,               // Type byte for definitions header
    0x00,               // Extension bytes modifier
    0x00_50,            // Definition type for fixed-size string (0x00_50)
    capacity (u32),
]
```

The definition can be referenced by its index in String values and type-specified values, for example, the following demonstrates a fixed-size String value "Hello..." with definition index `0x00_01`:

```asonb
[
    0x54,               // Type byte for fixed-size String
    0x00,               // Extension bytes modifier
    0x00_01,            // Index of the fixed-size string definition
    "Hello..."          // String data, padded with null bytes to fill the capacity
]
```

The following is another example demonstrates a type-specified List of fixed-size String (with definition index `0x00_01`) values:

```asonb
[
    0xA2,                       // Type byte for user defined type-specified List
    0x00,                       // Extension bytes modifier
    0x00_01,                    // Index of the fixed-size string definition
    [
        "Hello...",             // The first item, padded with null bytes to fill the capacity
        "World...",             // The second item, padded with null bytes to fill the capacity
        ...                     // More items
    ]
    0xFF, 0xFF, 0xFF, 0xFF,     // List end marker
]
```

Note that:

- The capacity should be multiple of 4.
- The definition only defines the capacity of the string, it does not define the actual length of the string data, thus the string data can be of any length up to the capacity. The actual length of the string data can be determined by finding the first null byte in the string data, or by using another value to store the actual length of the string data.

##### 2.3.7.4 Empty String

An empty string is represented as:

```asonb
[
    0x50,                       // Type byte for String
    0x00,                       // Extension bytes modifier
    0x00_04,                    // Indicates there are 4 null bytes
    0x00, 0x00, 0x00, 0x00,     // String data length (0 byte)
    0x00, 0x00, 0x00, 0x00      // Null bytes for both null-termination and alignment
]
```

The null bytes is also required for empty strings, because it provides a pointer to the string data when mapping the string data in memory to C-style null-terminated string.

There is also a compact form of empty string when the memory mapping is not required, which only contains the value prefix:

```asonb
[
    0x57, 0x00, 0x00, 0x00,     // Empty string
]
```

#### 2.3.8 Byte Data

Byte data is used for representing raw binary data (such as images, audio, or any other non-text data), it also can be used for representing fixed-length strings in data tables.

Format:

```asonb
[
    0x58,
    0x00,
    2-byte numbers of padding bytes (`u16`),
    4-byte length,
    data
]
```

For example, a byte data of `0xDE, 0xAD, 0xBE, 0xEF, 0x00` (5 bytes) is represented as:

```asonb
[
    0x58,                           // Type byte for byte data
    0x00,                           // Extension bytes modifier
    0x00_03,                        // Indicates there are 3 padding bytes
    0x05, 0x00, 0x00, 0x00,         // Byte data length (5 bytes)
    0xDE, 0xAD, 0xBE, 0xEF, 0x00,   // Byte data
    0x00, 0x00, 0x00                // Padding bytes for alignment
]
```

##### 2.3.8.1 Chunk Byte Data

```asonb
[
    0x59,                               // Data type for chunk byte data
    0x00,                               // Extension bytes modifier
    0x00, 0x00,                         // Extension bytes, not used for chunk byte data
    [
        4-byte length,                  // First chunk
        4-byte numbers of padding bytes,
        data,
        padding bytes,
    ]
    [                                   // Second chunk
        4-byte length,
        4-byte numbers of padding bytes,
        data,
        padding bytes,
    ]
    ... ,                               // More chunks
    0x00, 0x00, 0x00, 0x00              // End marker for chunk byte data
]
```

##### 2.3.8.2 Long Byte Data

If the length of byte data is greater than 0xFFFF_FFFF (4GB), the byte data must be encoded in another format:

```asonb
[
    0x5A,                               // Type byte for long byte data
    0x00,                               // Extension bytes modifier
    2-byte numbers of padding bytes,
    4-byte length low,                  // Low part of the `u64` length
    4-byte length high,                 // High part of the `u64` length
    data,
    padding bytes
]
```

##### 2.3.8.3 Fixed-size Byte Data

Fixed-size byte data has a fixed capacity (the max length), and the actual data is padded to fill the capacity, the capacity is defined in the definition header.

The fixed-size byte data definition header has the following format:

```asonb
[
    0xF1,                       // Type byte for definitions header
    1-byte alignment,
    0x00_58,                    // Definition type for fixed-size byte data (0x00_58)
    capacity (u32),
]
```

The 1-byte alignment indicates the required alignment for the data block of byte data , it can be 2 or 3, which means `2 power alignment` bytes alignment.

The definition can be referenced by its index in Byte Data values and type-specified values, for example, the following demonstrates a fixed-size Byte Data value "0x11, 0x13, 0x17..." with definition index `0x00_01`:

```asonb
[
    0x5C,                       // Type byte for fixed-size byte data
    0x00,                       // Extension bytes modifier
    0x00_01,                    // Index of the fixed-size byte data definition
    data,
    0x11, 0x13, 0x17, ...       // Actual data, padded with null bytes to fill the capacity
]
```

Note that:

- The capacity should be multiple of the alignment (which is `2 power alignment`).
- The definition only defines the capacity of the byte data, it does not define the actual length of the byte data, thus the byte data can be of any length up to the capacity.

##### 2.3.8.4 Empty Byte Data

`[0x5F, 0x00, 0x00, 0x00]`: Empty byte data

### 2.5 Objects

Objects are sequences of key-value pairs.

Format:

```asonb
[
    0x80,                       // Type byte for Object
    0x00,                       // Extension bytes modifier
    2-byte extension (`u16`),
    key-value pairs,
    0xFF, 0xFF, 0xFF, 0xFF,     // Object end marker
]
```

Individual Objects are terminated with an end marker (`0xFF_FF_FF_FF`), when Objects are enclosed in type-specified values, the value prefix and the end marker are omitted, only the value bytes are stored in raw format.

Key-value pair format:

`[key data, value data]`

Key is an identifier, but it is represented as a regular string in ASONB for simplicity. The value can be of any type, including primitive types and compound types.

For example, consider the following ASON object:

```ason
{
    id: 0x42
    name: "Alice"
}
```

Its binary representation in ASONB would be:

```asonb
[
    0x80,                       // Type byte for Object
    0x00,                       // Extension bytes modifier
    0x00_00,                    // Extension bytes, not used for regular Object
    [                           // The key "id"
        0x50, 0x00, 0x00_02,
        0x00_00_00_02,
        "id", 0x00, 0x00
    ],
    [                           // The value `i32` 0x42
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_42
    ],
    [                           // The key "name"
        0x50, 0x00, 0x00_04,
        0x00_00_00_04,
        "name",
        0x00, 0x00, 0x00, 0x00
    ],
    [                           // The value "Alice"
        0x50, 0x00, 0x00_03,
        0x00_00_00_05,
        "Alice",
        0x00, 0x00, 0x00
    ],
    0xFF, 0xFF, 0xFF, 0xFF      // Object end marker
]
```

#### 2.5.1 Field Names Predefined Object

To save space, field names of objects can be predefined in the definitions header, and then referenced by their index in the object key-value pairs.

The Object field names definition header has the following format:

```asonb
[
    0xF1,                       // Type byte for definitions header
    0x00,                       // Extension bytes modifier
    0x01_80,                    // Definition type for object field names (0x01_80)
    [                           // The field name definitions
        String value 1,
        String value 2,
        ...
    ]
]
```

The field name definitions are just a sequence of regular String values.

Consider there is an Object as:

```ason
{
    id: i32
    name: String
}
```

The following represents the field names definitions for the Object:

```asonb
[
    0xF1,                       // Type byte for definitions header
    0x00,                       // Extension bytes modifier
    0x01_80,                    // Definition type for object field names (0x01_80)
    [                           // The first field name "id"
        0x50, 0x00, 0x00_02,
        0x00_00_00_02,
        "id", 0x00, 0x00
    ],
    [                           // The second field name "name"
        0x50, 0x00, 0x00_04,
        0x00_00_00_04,
        "name",
        0x00, 0x00, 0x00, 0x00
    ],
]
```

Then the Object `{id: 0x42, name: "Alice"}` can be encoded as:

```asonb
[
    0x81,                       // Type byte for field names predefined Object
    0x00,                       // Extension bytes modifier
    0x00_00,                    // Index of the definition header (0 in this example)
    [0x00_00_00_00],            // Index of field name "id"
    [                           // The value `i32` 0x42
        0x15, 0x00, 0x00, 0x00,
        0x00_00_00_42
    ],
    [0x00_00_00_01],            // Index of field name "name"
    [                           // The value "Alice"
        0x50, 0x00, 0x00_03,
        0x00_00_00_05,
        "Alice",
        0x00, 0x00, 0x00
    ],
    0xFF, 0xFF, 0xFF, 0xFF      // Object end marker
]
```

Note that field names definitions only contain the names, the type of values is not included, thus the value prefix is still required for each value.

#### 2.5.2 Fixed-size Objects

If an Object has a fixed layout (the field names, the size and the type of values are predefined), it can be encoded in raw format without field names and value prefixes.

The fixed-size Object definition header has the following format:

```asonb
[
    0xF1,                       // Type byte for definitions header
    1-byte alignment,
    0x00_80,                    // Definition type for fixed-size object (0x00_80)
    field definitions           // The field definitions
]
```

The 1-byte alignment indicates the required alignment for the data block of the Object, it can be 2 or 3, which means `2 power alignment` bytes alignment. For example, if the Object contains any 64-bit numbers, the alignment should be set to 3 (8 bytes alignment). When the Object is encoded in raw format, some discrete padding bytes (0x00) may be inserted before the value bytes to achieve the required alignment.

The field definitions are a sequence of field definition, each field definition has the following format:

```asonb
[
    2-byte data type,
    2-byte data type modifier,
    regular String value for field name
]
```

The 2-byte data type is used to indicate the data type of the field value. If the data type is a fixed-size primitive type, the data type modifier is not used and should be set to `0x00_00`, if the data type is a predefined fixed-size value, the data type modifier is set to `0x00_01` and the data type is set to the index of the definition.

> Fixed-size Objects can only contain fixed-size values.

For example, consider an Object with a fixed layout as:

```ason
{
    id: i32
    name: 32-byte length String
}
```

To define this Object, we need to define a fixed-size String with capacity of 32 bytes first:

```asonb
[
    0xF1,                       // Type byte for definitions header
    0x00,                       // Extension bytes modifier
    0x00_50,                    // Definition type for fixed-size string (0x00_50)
    0x00_00_00_20               // Capacity of the fixed-size string (32 bytes)
]
```

Asuming the above definition is at index `0x00_00`, then we can define the fixed-size Object as:

```asonb
[
    0xF1,                       // Type byte for definitions header
    0x00,                       // Extension bytes modifier
    0x00_80,                    // Definition type for fixed-size object (0x00_80)
    [                           // The first field "id"
        0x00_15,                // Data type of the field value, in this case it is `i32`
        0x00_00,                // Data type modifier, set to `0x00_00` for fixed-size primitive types
        [                       // Regular String value for field name "id"
            0x50, 0x00, 0x00_02,
            0x00_00_00_02,
            "id", 0x00, 0x00
        ]
    ],
    [                           // The second field "name"
        0x00_00,                // Index of the predefined fixed-size String definition (0 in this example)
        0x00_01,                // Data type modifier, set to `0x00_01` for definition index
        [                       // Regular String value for field name "name"
            0x50, 0x00, 0x00_02,
            0x00_00_00_04,
            "name", 0x00, 0x00
        ]
    ],
]
```

Asuming the above fixed-size Object definition is at index `0x00_01`, then the Object `{id: 0x42, name: "Alice"}` can be encoded as:

```asonb
[
    0x82,                       // Type byte for fixed-size Object
    0x00,                       // Extension bytes modifier
    0x00_01,                    // The index of the fixed-size object definition (1 in this example)
    [
        0x00_00_00_42,          // The value of field "id" (i32 0x42) in raw format
        "Alice...",             // The value of field "name" (String "Alice") in raw format with null bytes
    ]
    0xFF, 0xFF, 0xFF, 0xFF      // Object end marker
]
```

Note that if this Object is enclosed in another type-specified value, the value prefix and the end marker are omitted, only the value bytes (i.e., the `[0x00_00_00_42, "Alice..."]`) are stored in raw format.

### 2.6 Tuples

Tuples are fixed-size collections of values, each value can be of any type.

Format:

```asonb
[
    0x90,
    0x00,

]
```

There is also a "Jump-out-byte" at the end of the tuple, and the length prefix indicates the total length of the values, excluding the Jump-out-byte.

For example, consider the following ASON tuple:

```ason
(0x2B, "foo", true)
```

Its binary representation in ASONB would be:

```asonb
[
    0x90,                               // Data type for tuple
    0x00_00_00_0F,                      // Total length of values (15 bytes)
    [0x15, 0x00_00_00_2B],              // The 32-bit signed integer 0x2B
    [0x70, 0x00_00_00_03, "foo", 0x00], // The string "foo"
    [0x41],                             // The boolean value true
    0x00                                // Jump-out-byte
]
```

#### 2.6.1 Variable-length tuples

Similar to variable-length objects, if a tuple contains any variable-length value (including chunk strings, chunk byte data), the tuple itself must be encoded in variable-length format:

`[0x98, values, 0x00]`

### 2.9 Lists

Lists are sequences of values of the same type.

Format:

`[0xA0, 4-byte items length, items, 0x00]`

For example, consider the following ASON list of `i32` integers:

```ason
[0x11, 0x13, 0x17, 0x19]
```

Its binary representation in ASONB would be:

```asonb
[
    0xA0,                   // Data type for list
    0x00_00_00_14,          // Total length of values (20 bytes)
    [0x15, 0x00_00_00_11],  // The first item (0x11)
    [0x15, 0x00_00_00_13],  // The second item (0x13)
    [0x15, 0x00_00_00_17],  // The third item (0x17)
    [0x15, 0x00_00_00_19],  // The fourth item (0x19)
    0x00                    // Jump-out-byte
]
```

#### 2.9.1 Variable-length lists

Format:

`[0xA8, items, 0x00]`

### 2.7 Named Lists

Named Lists are similar to Lists, but each value is associated with a name. All values in a Named List must be of the same type.

Format:

`[0xB0, 4-byte name-value pairs length, name-value pairs, 0x00]`

Format of each name-value pair:

`[name data, value data]`

The name can be of any type, although it is usually a string or number. Note that the name can not be a variable-length value (including chunk Strings, chunk byte data).

For example, consider the following ASON named list:

```ason
[
  "Red": 0xff0000,
  "Green": 0x00ff00,
  "Blue": 0x0000ff
]
```

Its binary representation in ASONB would be:

```asonb
[
    0xB0,                                   // Data type for named list
    0x00_00_00_2D,                          // Total length of name-value pairs (45 bytes)
    [0x70, 0x00_00_00_03, "Red", 0x00],     // The name "Red"
    [0x15, 0x00_FF_00_00],                  // The value 0xff0000
    [0x70, 0x00_00_00_05, "Green", 0x00],   // The name "Green"
    [0x15, 0x00_00_FF_00],                  // The value 0x00ff00
    [0x70, 0x00_00_00_04, "Blue", 0x00],    // The name "Blue"
    [0x15, 0x00_00_00_FF],                  // The value 0x0000ff
    0x00                                    // Jump-out-byte
]
```

#### 2.7.1 Variable-length named lists

`[0xB8, name-value pairs, 0x00]`

### 2.8 Enumerations

Enumerations represent a custom data type that consists of a type name and a set of named variants. A variant can have associated data, which can be of any type.

There are four kinds of variants in ASON:

- Variant without value (e.g. `Option::None`)
- Variant with single value (e.g. `Option::Some(123)`)
- Object-like variant (e.g. `Shape::Rectangle { width: 10, height: 20 }`)
- Tuple-like variant (e.g. `Color::RGB(255_u8, 127_u8, 63_u8)`)

#### 2.8.1 Variant without value

Variant without value is the simplest kind of variant, it only contains the type name and the variant name.

Format:

`[0xC0, type name, variant name]`

Type name and variant name are identifiers in ASON, but they are represented as regular strings in ASONB for simplicity.

For example, consider the following ASON enumeration:

```ason
Option::None
```

Its binary representation in ASONB would be:

```asonb
[
    0xC0,                                   // Data type
    [0x70, 0x00_00_00_06, "Option", 0x00],  // The type name "Option"
    [0x70, 0x00_00_00_04, "None", 0x00],    // The variant name "None"
]
```

#### 2.8.2 Variant with single value

Variant with single value can carry a single value of any type.

Format:

`[0xC1, type name, variant name, value data]`

For example, consider the following ASON enumeration:

```ason
Option::Some(0x42)                          // Integer value
Option::Some("Hello")                       // String value
Option::Some({id: 0x42, name: "Alice"})     // Object value
Option::Some((0x2B, "foo", true))           // Tuple value
Option::Some([0x11, 0x13, 0x17, 0x19])      // List value
```

Their binary representation in ASONB would be:

```asonb
// Integer value
[
    0xC1,                                   // Data type
    [0x70, 0x00_00_00_06, "Option", 0x00],  // The type name "Option"
    [0x70, 0x00_00_00_04, "Some", 0x00],    // The variant name "Some"
    [0x15, 0x00_00_00_42],                  // The 32-bit signed integer 0x42
]

// String value
[
    0xC1,                                   // Data type
    [0x70, 0x00_00_00_06, "Option", 0x00],  // The type name "Option"
    [0x70, 0x00_00_00_04, "Some", 0x00],    // The variant name "Some"
    [0x70, 0x00_00_00_05, "Hello", 0x00],   // The string value "Hello"
]

// Object value
[
    0xC1,                                       // Data type
    [0x70, 0x00_00_00_06, "Option", 0x00],      // The type name "Option"
    [0x70, 0x00_00_00_04, "Some", 0x00],        // The variant name "Some"
    [
        0x90,                                   // Data type for object
        0x00_00_00_22,                          // Total length of key-value pairs (34 bytes)
        [0x70, 0x00_00_00_02, "id", 0x00],      // The key "id"
        [0x15, 0x00_00_00_42],                  // The 32-bit signed integer 0x42
        [0x70, 0x00_00_00_04, "name", 0x00],    // The key "name"
        [0x70, 0x00_00_00_05, "Alice", 0x00],   // The value "Alice"
        0x00                                    // Jump-out-byte
    ],
]

// Tuple value
[
    0xC1,                                   // Data type
    [0x70, 0x00_00_00_06, "Option", 0x00],  // The type name "Option"
    [0x70, 0x00_00_00_04, "Some", 0x00],    // The variant name "Some"
    [
        0xA0,                               // Data type for tuple
        0x00_00_00_0F,                      // Total length of values (15 bytes)
        [0x15, 0x00_00_00_2B],              // The 32-bit signed integer 0x2B
        [0x70, 0x00_00_00_03, "foo", 0x00], // The string "foo"
        [0x41],                             // The boolean value true
        0x00                                // Jump-out-byte
    ]
]

// List value
[
    0xC1,                                   // Data type
    [0x70, 0x00_00_00_06, "Option", 0x00],  // The type name "Option"
    [0x70, 0x00_00_00_04, "Some", 0x00],    // The variant name "Some"
    [
        0xB0,                               // Data type for list
        0x00_00_00_14,                      // Total length of values (20 bytes)
        [0x15, 0x00_00_00_11],              // The first item (0x11)
        [0x15, 0x00_00_00_13],              // The second item (0x13)
        [0x15, 0x00_00_00_17],              // The third item (0x17)
        [0x15, 0x00_00_00_19],              // The fourth item (0x19)
        0x00                                // Jump-out-byte
    ]
]
```

#### 2.8.3 Object-like variant

Format:

`[0xC2, type name, variant name, 4-byte key-value pairs length, key-value pairs, 0x00]`

Given the following ASON enumeration:

```ason
User::Classic { id: 0x42, name: "Alice" }
```

Its binary representation in ASONB would be:

```asonb
[
    0xC2,
    [0x70, 0x00_00_00_04, "User", 0x00],    // The type name "User"
    [0x70, 0x00_00_00_07, "Classic", 0x00], // The variant name "Classic"

    0x00_00_00_22,                          // Total length of key-value pairs (34 bytes)

    [0x70, 0x00_00_00_02, "id", 0x00],      // The key "id"
    [0x15, 0x00_00_00_42],                  // The 32-bit signed integer 0x42
    [0x70, 0x00_00_00_04, "name", 0x00],    // The key "name"
    [0x70, 0x00_00_00_05, "Alice", 0x00],   // The value "Alice"

    0x00                                    // Jump-out-byte
]
```

There is also variable-length object-like variant, format:

`[0xCA, type name, variant name, key-value pairs, 0x00]`

#### 2.8.4 Tuple-like variant

Format:

`[0xC3, type name, variant name, 4-byte values length, values, 0x00]`

Given the following ASON enumeration:

```ason
Product::Book(0x2B, "foo", true)
```

Its binary representation in ASONB would be:

```asonb
[
    0xC3,
    [0x70, 0x00_00_00_07, "Product", 0x00], // The type name "Product"
    [0x70, 0x00_00_00_04, "Book", 0x00],    // The variant name "Book"

    0x00_00_00_0F,                          // Total length of values (15 bytes)

    [0x15, 0x00_00_00_2B],                  // The 32-bit signed integer 0x2B
    [0x70, 0x00_00_00_03, "foo", 0x00],     // The string "foo"
    [0x41],                                 // The boolean value true

    0x00                                    // Jump-out-byte
]
```

There is also variable-length tuple-like variant, format:

`[0xCB, type name, variant name, values, 0x00]`

### 2.9 Discrete Padding Bytes

In some cases, padding bytes (0x00) may be inserted between values to achieve alignment. These padding bytes are not part of the actual data, and they should be ignored when parsing the values. These bytes are called "discrete padding bytes".

The length of discrete padding bytes is variable (from 1 to any number of bytes), and it is determined by the alignment requirements of the values. For example, an ASONB document that only contains a single 64-bit integer value 0x42, the binary representation would be:

```asonb
[
    0x00, 0x00, 0x00, 0x00          // discrete padding bytes for 8-byte alignment
    [
        0x16, 0x00, 0x00, 0x00,     // Value prefix for 64-bit signed integer
        0x42, 0x00, 0x00, 0x00,     // The first 4 bytes of `i64`
        0x00, 0x00, 0x00, 0x00      // The second 4 bytes of `i64`
    ]
]
```

The 4 discrete padding bytes are preceded before the value to achieve 8-byte alignment for the value data block (i.e., the `[0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]`).

In some cases, the `i64` value data block happens to be 8-byte aligned, then the discrete padding bytes are not needed. For example, the following ASONB document represents the Tuple `(0x42_i32, 0x42_i64)`:

```asonb
[
    0x90, 0x00, 0x00, 0x00,         // Value prefix for Tuple
    [
        0x15, 0x00, 0x00, 0x00,     // Value prefix for 32-bit signed integer
        0x42, 0x00, 0x00, 0x00,     // The first item (0x42_i32)
    ],
    [
        0x16, 0x00, 0x00, 0x00,     // Value prefix for 64-bit signed integer
        0x42, 0x00, 0x00, 0x00,     // The first 4 bytes of `i64`
        0x00, 0x00, 0x00, 0x00      // The second 4 bytes of `i64`
    ]
]
```

In the above example, the value data block of the `i64` 0x42 is already 8-byte aligned, thus discrete padding bytes are not needed.

Another example shows an ASONB document that only contains a type-specified List of `i64` integers:

```asonb
[
    0x00, 0x00, 0x00, 0x00              // discrete padding bytes for 8-byte alignment
    [
        0xA1,                           // Type byte for type-specified list
        0x00,                           // Extension bytes modifiers
        0x00_16,                        // Data type of items (`i64`)
        [                               // The first item `i64` 0x11
            0x11, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00
        ],
        [                               // The second item `i64` 0x13
            0x13, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00
        ],
        [                               // The third item `i64` 0x17
            0x17, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00
        ]
    ]
]
```

The 4 discrete padding bytes are preceded before the List to achieve 8-byte alignment for the value data block of the List (i.e., the items of the List).

> The discrete padding bytes are not allowed to be inserted between the value prefix and the value data, also they are not allowed to be inserted between raw format values.

## 3 ASONB vs BSON, Protocol Buffers, CBOR, and other binary serialization formats

TODO

<!--
    Reference links

    - BSON: https://bsonspec.org/
    - Protocol Buffers: https://protobuf.dev/
    - MessagePack: https://msgpack.org/
    - CBOR: https://cbor.io/
    - Avro: https://avro.apache.org/
-->