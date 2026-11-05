//! Compile-fail coverage for Rust shapes that the Schema derive cannot encode.

use asonb::Schema;

#[derive(Schema)]
/// A unit struct has no nonempty Object or Tuple representation.
struct Unit;

#[derive(Schema)]
/// Arbitrary newtype structs are unsupported, even around a valid scalar.
struct Width(
    /// The wrapped integer does not make the outer Rust shape supported.
    u32,
);

#[derive(Schema)]
/// Tuple structs are distinct from supported ordinary Rust tuples.
struct RGB(
    /// First color component.
    u8,
    /// Second color component.
    u8,
    /// Third color component.
    u8,
);

#[derive(Schema)]
/// Empty tuple-struct syntax is still unsupported.
struct EmptyTuple();

#[derive(Schema)]
#[serde(transparent)]
/// Transparent support is restricted to named structs, not this tuple wrapper.
struct Transparent(
    /// A valid scalar cannot bypass the tuple-struct shape check.
    u32,
);

#[derive(Schema)]
/// A tuple-like variant must retain at least two serialized positions.
enum EmptyVariant {
    /// Empty tuple syntax is not the same as a supported unit variant.
    Empty(),
}

#[derive(Schema)]
/// Skipping fields must not produce an invalid one-position tuple-like variant.
enum SkippedVariant {
    /// The original two-position shape becomes invalid after the skip is applied.
    One(
        /// The sole position that remains in the serialized payload.
        u32,
        #[serde(skip)]
        /// Removed from the schema as well as from serialized values.
        u32,
    ),
}

/// Only compilation and its diagnostics matter; this program is never run.
fn main() {}