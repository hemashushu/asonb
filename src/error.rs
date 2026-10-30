// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::fmt::{self, Display};

#[derive(Debug, PartialEq, Clone)]
pub enum AsonbError {
    Message(String),
}

impl Display for AsonbError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AsonbError::Message(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for AsonbError {}

impl serde::de::Error for AsonbError {
    fn custom<T: Display>(msg: T) -> Self {
        AsonbError::Message(msg.to_string())
    }
}

impl serde::ser::Error for AsonbError {
    fn custom<T: Display>(msg: T) -> Self {
        AsonbError::Message(msg.to_string())
    }
}
