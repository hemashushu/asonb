// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

//! Small fixtures independent of the high-level serializer, and I/O sources
//! that deliberately fragment or fail so tests exercise the actual contracts.
use crate::{DocumentSchema, Type, binary::HEADER};
use std::io::{self, Read, Write};

pub fn document(ty: Type, payload: &[u8]) -> Vec<u8> {
    let mut bytes = HEADER.to_vec();
    DocumentSchema::new(ty).write(&mut bytes).unwrap();
    bytes.extend_from_slice(payload);
    bytes
}

pub struct FaultReader<'a> {
    pub bytes: &'a [u8],
    pub position: usize,
    pub fail_at: usize,
    pub interrupt: bool,
}
impl<'a> FaultReader<'a> {
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

pub struct FaultWriter {
    pub bytes: Vec<u8>,
    pub fail_at: usize,
    pub interrupt: bool,
    pub zero: bool,
}
impl Default for FaultWriter {
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
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
