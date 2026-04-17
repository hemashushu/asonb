// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::{io::Read, marker::PhantomData};

use serde::de;

use crate::error::AsonError;

pub fn de_from_reader<'a, T, R>(_reader: &mut R) -> Result<T, AsonError>
where
    T: de::DeserializeOwned,
    R: Read + 'a,
{
    todo!()
}

pub fn list_from_reader<'a, T, R>(_reader: &mut R) -> Result<ListDeserializer<'a, T, R>, AsonError>
where
    T: de::DeserializeOwned,
    R: Read + 'a,
{
    todo!()
}

pub struct Deserializer<'a, R>
where
    R: Read + 'a,
{
    _upstream: &'a mut R,
}

pub struct ListDeserializer<'a, T, R>
where
    R: Read + 'a,
    T: de::DeserializeOwned,
{
    _deserializer: Deserializer<'a, R>,
    _marker: PhantomData<T>,
}
