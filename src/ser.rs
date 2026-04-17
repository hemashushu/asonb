// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::{io::Write, marker::PhantomData};

use serde::Serialize;

use crate::error::AsonError;

pub fn ser_to_writer<T, W: Write>(_value: &T, _writer: &mut W) -> Result<(), AsonError>
where
    T: Serialize,
{
    todo!()
}

pub fn list_to_writer<T, W>(_writer: &mut W) -> ListSerializer<'_, W, T>
where
    W: Write,
{
    todo!()
}

pub struct Serializer<'a, W>
where
    W: Write + 'a,
{
    _upstream: &'a mut W,
}

pub struct ListSerializer<'a, W, T>
where
    W: Write + 'a,
{
    _serializer: Serializer<'a, W>,
    _marker: PhantomData<T>,
}
