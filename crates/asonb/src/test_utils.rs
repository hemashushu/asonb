// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Small fixtures independent of the high-level serializer, and I/O sources
//! that deliberately fragment or fail so tests exercise the actual contracts.
use crate::{DocumentSchema, Type, binary::HEADER};
use std::io::{self, Read, Write};

/// Builds fixture bytes without using the high-level value serializer.
///
/// `ty` is written as an inline root schema; `payload` is appended verbatim,
/// including deliberately malformed bytes. Schema writing is expected to work
/// and panics if it does not. This helper is available only in library tests.
///
/// ```text
/// let bytes = document(Type::I32, &42i32.to_le_bytes());
/// ```
pub fn document(ty: Type, payload: &[u8]) -> Vec<u8> {
    let mut bytes = HEADER.to_vec();
    DocumentSchema::new(ty).write(&mut bytes).unwrap();
    bytes.extend_from_slice(payload);
    bytes
}

/// A one-byte-at-a-time source with an initial interruption and optional failure.
pub struct FaultReader<'a> {
    /// Fixture bytes to deliver; the reader never borrows a mutable buffer.
    pub bytes: &'a [u8],
    /// Index of the next byte to return.
    pub position: usize,
    /// Fail before reading at this index; usize::MAX disables injected failure.
    pub fail_at: usize,
    /// Return Interrupted once before any other nonempty-read behavior.
    pub interrupt: bool,
}
impl<'a> FaultReader<'a> {
    /// Starts reading `bytes` at zero, with one interruption and no hard failure.
    ///
    /// Change `fail_at` after construction to test a specific error boundary.
    /// Empty output buffers remain legal and do not consume the interruption.
    ///
    /// ```text
    /// let mut reader = FaultReader::new(&bytes);
    /// reader.fail_at = 5;
    /// ```
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            fail_at: usize::MAX,
            interrupt: true,
        }
    }
}
impl Read for FaultReader<'_> {
    /// Returns at most one byte so callers must handle short reads and retries.
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.interrupt {
            self.interrupt = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.position >= self.fail_at {
            return Err(io::Error::other("injected read failure"));
        }
        if self.position >= self.bytes.len() {
            return Ok(0);
        }
        output[0] = self.bytes[self.position];
        self.position += 1;
        Ok(1)
    }
}

/// A collecting sink that forces short writes and configurable write failures.
pub struct FaultWriter {
    /// Successfully written bytes, useful for checking partial output.
    pub bytes: Vec<u8>,
    /// Fail once this many bytes have been accepted; usize::MAX disables failure.
    pub fail_at: usize,
    /// Return Interrupted on the next nonempty write, then clear the flag.
    pub interrupt: bool,
    /// Return zero on nonempty writes, exercising write_all's WriteZero handling.
    pub zero: bool,
}
impl Default for FaultWriter {
    /// Creates a short-writing sink with one interruption and no permanent error.
    fn default() -> Self {
        Self {
            bytes: Vec::new(),
            fail_at: usize::MAX,
            interrupt: true,
            zero: false,
        }
    }
}
impl Write for FaultWriter {
    /// Accepts one byte, unless interruption, WriteZero, or failure is configured.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if self.interrupt {
            self.interrupt = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.zero {
            return Ok(0);
        }
        if self.bytes.len() >= self.fail_at {
            return Err(io::Error::other("injected write failure"));
        }
        self.bytes.push(bytes[0]);
        Ok(1)
    }
    /// Does nothing because accepted bytes are already visible in memory.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
