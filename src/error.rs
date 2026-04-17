// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::fmt::{self, Display};

#[derive(Debug, PartialEq, Clone)]
pub enum AsonError {
    Message(String),
}

impl Display for AsonError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AsonError::Message(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for AsonError {}

impl serde::de::Error for AsonError {
    fn custom<T: Display>(msg: T) -> Self {
        AsonError::Message(msg.to_string())
    }
}

impl serde::ser::Error for AsonError {
    fn custom<T: Display>(msg: T) -> Self {
        AsonError::Message(msg.to_string())
    }
}
