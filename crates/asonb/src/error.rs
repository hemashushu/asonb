// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::fmt::{self, Display};

/// An invalid schema, an unsupported value, malformed bytes, or an I/O failure.
///
/// Offset-bearing variants count bytes from the start of the document, starting
/// at zero. Errors raised by a user-defined Serde implementation may have no
/// offset because Serde reports a message without a position.
#[derive(Debug, PartialEq, Clone)]
pub enum AsonbError {
    /// A validation or Serde error without a known byte position.
    Message(
        /// A human-readable explanation of what went wrong.
        String,
    ),
    /// Invalid input or an I/O failure at a zero-based byte offset.
    MessageWithOffset(
        /// A human-readable explanation, without the formatted offset suffix.
        String,
        /// The byte position at which the error was detected.
        u64,
    ),
    /// EOF occurred while a required prefix or value was incomplete.
    UnexpectedEndOfDocument(
        /// The byte position where more input was needed.
        u64,
    ),
}

impl Display for AsonbError {
    /// Formats the message and includes a byte position when one is available.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AsonbError::Message(msg) => f.write_str(msg),
            AsonbError::MessageWithOffset(msg, offset) => {
                write!(f, "{msg} (byte offset {offset})")
            }
            AsonbError::UnexpectedEndOfDocument(offset) => {
                write!(f, "Unexpected end of document at byte offset {offset}.")
            }
        }
    }
}

impl std::error::Error for AsonbError {}

impl serde::de::Error for AsonbError {
    /// Preserves a Serde decoding error as text when no input offset is known.
    fn custom<T: Display>(msg: T) -> Self {
        AsonbError::Message(msg.to_string())
    }
}

impl serde::ser::Error for AsonbError {
    /// Preserves a Serde encoding error without inventing a byte position.
    fn custom<T: Display>(msg: T) -> Self {
        AsonbError::Message(msg.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Locks down user-facing formatting and Serde's custom-error conversion.
    fn test_error_messages() {
        assert_eq!(AsonbError::Message("invalid".into()).to_string(), "invalid");
        assert_eq!(
            AsonbError::MessageWithOffset("invalid".into(), 7).to_string(),
            "invalid (byte offset 7)"
        );
        assert_eq!(
            AsonbError::UnexpectedEndOfDocument(9).to_string(),
            "Unexpected end of document at byte offset 9."
        );
        let error: AsonbError = serde::de::Error::custom("custom");
        assert_eq!(error, AsonbError::Message("custom".into()));
    }
}
