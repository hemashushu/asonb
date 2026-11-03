// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Checked binary I/O shared by the schema, value, and Serde layers.

use std::{
    borrow::Cow,
    io::{self, Read, Write},
};

use crate::AsonbError;

pub(crate) const HEADER: [u8; 8] = [0, b'A', b'S', b'O', b'N', b'B', 0, 1];

pub(crate) fn write_all(writer: &mut impl Write, bytes: &[u8]) -> Result<(), AsonbError> {
    writer
        .write_all(bytes)
        .map_err(|error| AsonbError::Message(error.to_string()))
}

pub(crate) fn write_u32(writer: &mut impl Write, value: u64) -> Result<(), AsonbError> {
    let value =
        u32::try_from(value).map_err(|_| AsonbError::Message("Length exceeds u32.".into()))?;
    write_all(writer, &value.to_le_bytes())
}

/// Payloads borrow only from a slice source. A general Read source cannot
/// lend bytes with the lifetime of the document.
pub(crate) trait Input<'de> {
    fn offset(&self) -> u64;
    fn byte(&mut self) -> Result<Option<u8>, AsonbError>;
    fn payload(&mut self, length: u64) -> Result<Cow<'de, [u8]>, AsonbError>;

    fn exact<const SIZE: usize>(&mut self) -> Result<[u8; SIZE], AsonbError> {
        let mut bytes = [0; SIZE];
        for byte in &mut bytes {
            *byte = self
                .byte()?
                .ok_or(AsonbError::UnexpectedEndOfDocument(self.offset()))?;
        }
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32, AsonbError> {
        Ok(u32::from_le_bytes(self.exact()?))
    }
    fn u64(&mut self) -> Result<u64, AsonbError> {
        Ok(u64::from_le_bytes(self.exact()?))
    }

    /// EOF is accepted before the first byte, never inside the prefix.
    fn chunk(&mut self, root: bool) -> Result<u32, AsonbError> {
        match self.byte()? {
            None if root => Ok(0),
            None => Err(AsonbError::UnexpectedEndOfDocument(self.offset())),
            Some(first) => {
                let tail = self.exact::<3>()?;
                Ok(u32::from_le_bytes([first, tail[0], tail[1], tail[2]]))
            }
        }
    }

    fn header(&mut self) -> Result<(), AsonbError> {
        for (index, expected) in HEADER.iter().enumerate() {
            let offset = self.offset();
            let byte = self.byte()?.ok_or_else(|| {
                AsonbError::MessageWithOffset(
                    if index == 0 {
                        "Expected an ASONB document, but the input is empty.".into()
                    } else {
                        "Incomplete ASONB header: the input ended before the header was complete."
                            .into()
                    },
                    offset,
                )
            })?;
            if byte != *expected {
                let message = match index {
                    6 => format!(
                        "Unsupported ASONB feature flags: 0x{byte:02X}; expected 0x00."
                    ),
                    7 => format!("Unsupported ASONB version: {byte}; expected version 1."),
                    _ => "Expected an ASONB document, but the input does not match the ASONB signature."
                        .into(),
                };
                return Err(AsonbError::MessageWithOffset(message, offset));
            }
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsonbError> {
        let offset = self.offset();
        if self.byte()?.is_some() {
            return Err(AsonbError::MessageWithOffset(
                "Unexpected bytes after the root value.".into(),
                offset,
            ));
        }
        Ok(())
    }
}

pub(crate) struct SliceInput<'de> {
    pub remaining: &'de [u8],
    offset: u64,
}

impl<'de> SliceInput<'de> {
    pub fn new(bytes: &'de [u8]) -> Self {
        Self {
            remaining: bytes,
            offset: 0,
        }
    }
}

impl<'de> Input<'de> for SliceInput<'de> {
    fn offset(&self) -> u64 {
        self.offset
    }
    fn byte(&mut self) -> Result<Option<u8>, AsonbError> {
        match self.remaining.split_first() {
            Some((&byte, tail)) => {
                self.remaining = tail;
                self.offset += 1;
                Ok(Some(byte))
            }
            None => Ok(None),
        }
    }
    fn payload(&mut self, length: u64) -> Result<Cow<'de, [u8]>, AsonbError> {
        let size = payload_size(length)?;
        if size > self.remaining.len() {
            return Err(AsonbError::UnexpectedEndOfDocument(
                self.offset + self.remaining.len() as u64,
            ));
        }
        let (payload, tail) = self.remaining.split_at(size);
        self.remaining = tail;
        self.offset += length;
        Ok(Cow::Borrowed(payload))
    }
}

pub(crate) struct ReaderInput<R> {
    pub reader: R,
    offset: u64,
}

impl<R> ReaderInput<R> {
    pub fn new(reader: R) -> Self {
        Self { reader, offset: 0 }
    }
}

impl<'de, R: Read> Input<'de> for ReaderInput<R> {
    fn offset(&self) -> u64 {
        self.offset
    }
    fn byte(&mut self) -> Result<Option<u8>, AsonbError> {
        let mut byte = [0];
        loop {
            match self.reader.read(&mut byte) {
                Ok(0) => return Ok(None),
                Ok(_) => {
                    self.offset += 1;
                    return Ok(Some(byte[0]));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(AsonbError::MessageWithOffset(
                        error.to_string(),
                        self.offset,
                    ));
                }
            }
        }
    }
    fn exact<const SIZE: usize>(&mut self) -> Result<[u8; SIZE], AsonbError> {
        let mut bytes = [0; SIZE];
        let mut filled = 0;
        while filled < SIZE {
            match self.reader.read(&mut bytes[filled..]) {
                Ok(0) => return Err(AsonbError::UnexpectedEndOfDocument(self.offset)),
                Ok(count) => {
                    filled += count;
                    self.offset += count as u64;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(AsonbError::MessageWithOffset(
                        error.to_string(),
                        self.offset,
                    ));
                }
            }
        }
        Ok(bytes)
    }
    fn payload(&mut self, length: u64) -> Result<Cow<'de, [u8]>, AsonbError> {
        let size = payload_size(length)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|error| AsonbError::Message(error.to_string()))?;
        bytes.resize(size, 0);
        let mut filled = 0;
        while filled < size {
            match self.reader.read(&mut bytes[filled..]) {
                Ok(0) => return Err(AsonbError::UnexpectedEndOfDocument(self.offset)),
                Ok(count) => {
                    filled += count;
                    self.offset += count as u64;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    return Err(AsonbError::MessageWithOffset(
                        error.to_string(),
                        self.offset,
                    ));
                }
            }
        }
        Ok(Cow::Owned(bytes))
    }
}

/// A payload is constrained only by the platform.
pub(crate) fn payload_size(length: u64) -> Result<usize, AsonbError> {
    usize::try_from(length)
        .map_err(|_| AsonbError::Message("Payload length does not fit usize.".into()))
}

/// Fallible storage for an unknown-length value container. There is no
/// artificial capacity quota; Vec still checks platform capacity and allocation.
pub(crate) struct ValueBuffer {
    pub bytes: Vec<u8>,
}
impl Write for ValueBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes
            .try_reserve(bytes.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_and_truncation() {
        for end in 0..HEADER.len() {
            let message = if end == 0 {
                "Expected an ASONB document, but the input is empty."
            } else {
                "Incomplete ASONB header: the input ended before the header was complete."
            };
            assert_header_error(&HEADER[..end], message, end as u64);
        }
        SliceInput::new(&HEADER).header().unwrap();
        for index in 0..HEADER.len() {
            let mut bytes = HEADER;
            bytes[index] ^= 1;
            let message = match index {
                6 => "Unsupported ASONB feature flags: 0x01; expected 0x00.",
                7 => "Unsupported ASONB version: 0; expected version 1.",
                _ => {
                    "Expected an ASONB document, but the input does not match the ASONB signature."
                }
            };
            assert_header_error(&bytes, message, index as u64);
        }
    }

    fn assert_header_error(bytes: &[u8], message: &str, offset: u64) {
        let expected = AsonbError::MessageWithOffset(message.into(), offset);
        assert_eq!(SliceInput::new(bytes).header().unwrap_err(), expected);
        assert_eq!(ReaderInput::new(bytes).header().unwrap_err(), expected);
        assert_eq!(crate::de_from_slice::<u8>(bytes).unwrap_err(), expected);
        assert_eq!(crate::de_from_reader::<u8, _>(bytes).unwrap_err(), expected);
        assert_eq!(
            crate::list_stream_reader::<u8, _>(bytes).err().unwrap(),
            expected
        );
        assert_eq!(
            expected.to_string(),
            format!("{message} (byte offset {offset})")
        );
    }

    #[test]
    fn test_non_asonb_header() {
        for bytes in [b"x".as_slice(), b"{\"name\": \"foo\"}", b"\x00JSON"] {
            assert_header_error(
                bytes,
                "Expected an ASONB document, but the input does not match the ASONB signature.",
                if bytes[0] == 0 { 1 } else { 0 },
            );
        }
    }

    #[test]
    fn test_prefix_boundaries() {
        assert_eq!(SliceInput::new(&[]).chunk(true).unwrap(), 0);
        assert!(SliceInput::new(&[]).chunk(false).is_err());
        for end in 1..4 {
            assert!(SliceInput::new(&[1, 0, 0][..end]).chunk(true).is_err());
        }
        let mut output = Vec::new();
        write_u32(&mut output, u32::MAX as u64).unwrap();
        assert_eq!(output, [255; 4]);
        assert!(write_u32(&mut output, u32::MAX as u64 + 1).is_err());
    }

    #[test]
    fn test_owned_and_borrowed_payloads() {
        let bytes = [1, 2, 3];
        let mut input = SliceInput::new(&bytes);
        assert!(matches!(input.payload(3).unwrap(), Cow::Borrowed(_)));
        input.finish().unwrap();
        let mut input = ReaderInput::new(bytes.as_slice());
        assert_eq!(input.payload(3).unwrap().as_ref(), &bytes);
        input.finish().unwrap();
        assert!(ReaderInput::new(bytes.as_slice()).payload(4).is_err());
    }
}
