// Copyright (c) 2026 Hemashushu <hippospark@gmail.com>, All rights reserved.
//
// This Source Code Form is subject to the terms of
// the Mozilla Public License version 2.0 and additional exceptions.
// For more details, see the LICENSE, LICENSE.additional, and CONTRIBUTING files.

use std::fmt::{self, Display};

/// An invalid schema, an unsupported value, malformed bytes, or an I/O failure.
///
/// Offset-bearing variants count bytes from the start of the document, starting
/// at zero, even when the underlying reader was already positioned within a
/// larger source. A position identifies where a problem was detected or the
/// relevant payload began; it is not necessarily the first invalid byte or a
/// safe point at which to resume reading.
///
/// Serde custom errors, schema validation failures, and write-side I/O failures
/// generally use [`Self::Message`] because no input position is available.
/// Underlying I/O errors are stored as text, not retained as an `io::Error`
/// source or `ErrorKind`. An invalid or incomplete document header uses
/// [`Self::MessageWithOffset`], while a truncated payload or length prefix
/// normally uses [`Self::UnexpectedEndOfDocument`].
///
/// This error does not represent an application's `Result::Err` payload:
/// [`crate::result_stream_reader`] returns that as [`crate::ResultStreamReader::Err`].
/// A failed read can consume input, and a failed write can leave partial output;
/// neither operation is rolled back.
///
/// # Examples
///
/// ```
/// let mut bytes = asonb::ser_to_bytes(&42u32)?;
/// bytes.pop();
/// match asonb::de_from_slice::<u32>(&bytes) {
///     Err(asonb::AsonbError::UnexpectedEndOfDocument(offset)) => {
///         assert_eq!(offset, bytes.len() as u64);
///     }
///     _ => panic!("expected a truncated integer"),
/// }
/// # Ok::<(), asonb::AsonbError>(())
/// ```
#[derive(Debug, PartialEq, Clone)]
pub enum AsonbError {
    /// A validation, Serde, allocation, or write error without a byte position.
    Message(
        /// A human-readable explanation of what went wrong.
        String,
    ),
    /// Invalid input or a read-side I/O failure with a zero-based byte offset.
    MessageWithOffset(
        /// A human-readable explanation, without the formatted offset suffix.
        String,
        /// Where the error was detected, or the start of the offending payload.
        u64,
    ),
    /// EOF occurred while a required schema entry, prefix, or value was incomplete.
    ///
    /// Clean EOF between chunks can terminate a MultiChunk root value, so that
    /// case is not an error. Nested MultiChunk values require an explicit marker.
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
