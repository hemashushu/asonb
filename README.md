# ASON Binary Format

ASONB (XiaoXuan Script Object Notation Binary) is a compact, efficient binary format for representing ASON data structures. It is designed for easy manipulation and fast data interchange in various applications.

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=4 orderedList=false} -->

<!-- code_chunk_output -->

- [Schema](#schema)
  - [Overview](#overview)
  - [Primitive Data Types](#primitive-data-types)
    - [Integers](#integers)
    - [Floating Point Numbers](#floating-point-numbers)
    - [Boolean Values](#boolean-values)
    - [Characters](#characters)
    - [Strings](#strings)
    - [Date and Time](#date-and-time)
  - [Objects](#objects)
  - [Lists](#lists)
  - [Named Lists](#named-lists)
  - [Tuples](#tuples)
  - [Variants](#variants)
    - [No value variant](#no-value-variant)
    - [Single value variant](#single-value-variant)
    - [Object-like variant](#object-like-variant)
    - [Tuple-like variant](#tuple-like-variant)

<!-- /code_chunk_output -->

## Schema

### Overview

Primitive types (such as numbers and strings) in ASONB are encoded as:

`[data type, subtype, length, data]`

- **data type**: 1 byte indicating the type of data
- **subtype**: 1 byte indicating the subtype
- **length**: 4-byte unsigned integer (little-endian) specifying the length of the data
- **data**: the actual data, variable length

All numeric types use little-endian format. Strings are UTF-8 encoded.

Compound types (arrays, objects, etc.) are encoded as:

`[data type, length, data, end marker]`

- **data type**: 1 byte indicating the type
- **length**: 4-byte unsigned integer (little-endian) specifying the length of the data (the end marker is not included in this length)
- **data**: the contained structures or values
- **end marker**: 1 byte (`0xFF`) marking the end of the structure, aiding parsing

### Primitive Data Types

#### Integers

- `[0x01, 0b0000_0010, 1-byte data]`: 8-bit signed integer
- `[0x01, 0b0000_0011, 1-byte data]`: 8-bit unsigned integer
- `[0x01, 0b0000_0100, 2-byte data]`: 16-bit signed integer
- `[0x01, 0b0000_0101, 2-byte data]`: 16-bit unsigned integer
- `[0x01, 0b0000_1000, 4-byte data]`: 32-bit signed integer
- `[0x01, 0b0000_1001, 4-byte data]`: 32-bit unsigned integer
- `[0x01, 0b0001_0000, 8-byte data]`: 64-bit signed integer
- `[0x01, 0b0001_0001, 8-byte data]`: 64-bit unsigned integer

#### Floating Point Numbers

- `[0x02, 0b0000_0100, 4-byte data]`: 32-bit float (IEEE 754)
- `[0x02, 0b0000_1000, 8-byte data]`: 64-bit float (IEEE 754)

#### Boolean Values

- `[0x03, 0x00]`: false
- `[0x03, 0x01]`: true

#### Characters

- `[0x04, 4-byte data]`: 32-bit code point

#### Strings

- `[0x05, 4-byte length, string data]`: UTF-8 string

#### Date and Time

- `[0x06, 8-byte data]`: 64-bit integer (seconds since epoch)

### Objects

Objects are sequences of key-value pairs. Each key is a string; each value can be any type.

Format:

`[0x10, 4-byte length, key-value pairs, 0xFF]`

Each key-value pair:

`[key data, value data]`

Example:

```ason
{
  name: "John Smith",
  age: 30_u8
}
```

Binary representation:

```asonb
[0x10, 0x00_00_00_23,
    [0x05, 0x00_00_00_04, "name"],
    [0x05, 0x00_00_00_0A, "John Smith"],
    [0x05, 0x00_00_00_03, "age"],
    [0x01, 0b0000_0011, 0x1E],
 0xFF]
```

### Lists

Lists are sequences of values of the same type.

Format:

`[0x11, 4-byte length, values, 0xFF]`

Example numeric list:

```ason
[11, 13, 17, 19]
```

Binary representation:

```asonb
[0x11, 0x00_00_00_18,
    [0x01, 0b0000_1000, 0x00_00_00_0B],
    [0x01, 0b0000_1000, 0x00_00_00_0D],
    [0x01, 0b0000_1000, 0x00_00_00_11],
    [0x01, 0b0000_1000, 0x00_00_00_13],
 0xFF]
```

Example string list:

```ason
["Alice", "Bob", "Carol"]
```

Binary representation:

```asonb
[0x11, 0x00_00_00_1C,
    [0x05, 0x00_00_00_05, "Alice"],
    [0x05, 0x00_00_00_03, "Bob"],
    [0x05, 0x00_00_00_05, "Carol"],
 0xFF]
```

### Named Lists

Named lists are similar to lists, but each value is associated with a name. All names and values must be of the same type within a list.

Format:

`[0x12, 4-byte length, name-value pairs, 0xFF]`

Each pair:

`[name data, value data]`

Example:

```ason
[
  "Alice": 30_u8,
  "Bob": 25_u8,
  "Carol": 28_u8
]
```

Binary representation:

```asonb
[0x12, 0x00_00_00_2C,
    [0x05, 0x00_00_00_05, "Alice"],
    [0x01, 0b0000_0011, 0x1E],
    [0x05, 0x00_00_00_03, "Bob"],
    [0x01, 0b0000_0011, 0x19],
    [0x05, 0x00_00_00_05, "Carol"],
    [0x01, 0b0000_0011, 0x1C],
 0xFF]
```

### Tuples

Tuples are fixed-size collections of values, each of any type.

Format:

`[0x13, 4-byte length, values, 0xFF]`

Example:

```ason
(42, "Hello", true)
```

Binary representation:

```asonb
[0x13, 0x00_00_00_1C,
    [0x01, 0b0000_1000, 0x00_00_00_2A],
    [0x05, 0x00_00_00_05, "Hello"],
    [0x03, 0x01],
 0xFF]
```

### Variants

Variants represent values that can be one of several types. Each variant has a type name, a member name, and an optional value.

There are four styles in ASON:

- No value (e.g. `Option::None`)
- Single value (e.g. `Option::Some(ANY_VALUE)`)
- Object-like (e.g. `Shape::Rectangle { width: 10, height: 20 }`)
- Tuple-like (e.g. `Color::RGB(255_u8, 127_u8, 63_u8)`)

Format:

`[0x14, 1-byte style, 4-byte length, type name, member name, value data, 0xFF]`

- **style**: 1 byte (0 = no value, 1 = single value, 2 = object-like, 3 = tuple-like)
- **length**: 4-byte unsigned integer (little-endian)
- **type name**: string
- **member name**: string
- **value data**: optional, any type

#### No value variant

```ason
Option::None
```

Binary:

```asonb
[0x14, 0x00, 0x00_00_00_14,
    [0x05, 0x00_00_00_06, "Option"],
    [0x05, 0x00_00_00_04, "None"],
 0xFF]
```

#### Single value variant

```ason
Option::Some(42)
```

Binary:

```asonb
[0x14, 0x01, 0x00_00_00_1A,
    [0x05, 0x00_00_00_06, "Option"],
    [0x05, 0x00_00_00_04, "Some"],
    [0x01, 0b0000_1000, 0x00_00_00_2A],
 0xFF]
```

#### Object-like variant

```ason
Shape::Rectangle { width: 10, height: 20 }
```

Binary:

```asonb
[0x14, 0x02, 0x00_00_00_3F,
    [0x05, 0x00_00_00_05, "Shape"],
    [0x05, 0x00_00_00_09, "Rectangle"],
    [0x10, 0x00_00_00_21,
        [0x05, 0x00_00_00_05, "width"],
        [0x01, 0b0000_1000, 0x00_00_00_0A],
        [0x05, 0x00_00_00_06, "height"],
        [0x01, 0b0000_1000, 0x00_00_00_14],
     0xFF],
 0xFF]
```

Note: The value of an object-like variant is encoded as an object. This is similar to a single-value variant with an object value, except for the `style` byte (`0x02` for object-like, `0x01` for single-value). This design simplifies parsing.

#### Tuple-like variant

```ason
Color::RGB(255_u8, 127_u8, 63_u8)
```

Binary:

```asonb
[0x14, 0x03, 0x00_00_00_2A,
    [0x05, 0x00_00_00_05, "Color"],
    [0x05, 0x00_00_00_03, "RGB"],
    [0x13, 0x00_00_00_12,
        [0x01, 0b0000_1001, 0x00_00_00_FF],
        [0x01, 0b0000_1001, 0x00_00_00_7F],
        [0x01, 0b0000_1001, 0x00_00_00_3F],
     0xFF],
 0xFF]
```

Again, this is similar to a single-value variant with a tuple value, except for the `style` byte.

