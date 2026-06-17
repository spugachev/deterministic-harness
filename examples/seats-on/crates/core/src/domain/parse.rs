//! Booking request parsing — turn untrusted text into a typed [`Booking`].
//!
//! `parse_booking` is **total and panic-free for ANY input**: every `&str`
//! (empty, unicode, gigantic numbers, embedded NULs) maps to either `Ok` or a
//! typed [`ParseError`]. No `unwrap`, no indexing, no slicing — clippy's
//! restriction lints forbid them and a proptest law proves the totality.

/// A parsed booking request: hold `qty` seats for section `section`.
///
/// Wire format: `"<section>:<qty>"`, e.g. `"A:3"`. `section` is a non-empty
/// run of bytes with no `:`; `qty` is a base-10 `u32`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Booking {
    /// The seating section identifier (non-empty, no separator).
    pub section: String,
    /// Number of seats requested.
    pub qty: u32,
}

/// Why a booking string could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The input was empty or all whitespace.
    Empty,
    /// The input had no `:` separator, or had more than one.
    MalformedSeparator,
    /// The section part was empty.
    EmptySection,
    /// The quantity part was empty, non-numeric, or out of `u32` range.
    InvalidQuantity,
}

/// Parse a booking request from untrusted text. Total: never panics.
///
/// # Errors
///
/// Returns a [`ParseError`] variant describing the first problem found —
/// empty input, a missing/duplicated `:`, an empty section, or a quantity that
/// is not a base-10 `u32`.
pub fn parse_booking(input: &str) -> Result<Booking, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    // Exactly one separator. `split_once` is panic-free; a second `:` in the
    // remainder makes the input ambiguous, so reject it.
    let (section, qty_str) = trimmed
        .split_once(':')
        .ok_or(ParseError::MalformedSeparator)?;
    if qty_str.contains(':') {
        return Err(ParseError::MalformedSeparator);
    }

    let section = section.trim();
    if section.is_empty() {
        return Err(ParseError::EmptySection);
    }

    let qty = qty_str
        .trim()
        .parse::<u32>()
        .map_err(|_| ParseError::InvalidQuantity)?;

    Ok(Booking {
        section: section.to_owned(),
        qty,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::any;

    #[test]
    fn parses_well_formed() {
        assert_eq!(
            parse_booking("A:3"),
            Ok(Booking {
                section: "A".to_owned(),
                qty: 3
            })
        );
        assert_eq!(
            parse_booking("  VIP : 0 "),
            Ok(Booking {
                section: "VIP".to_owned(),
                qty: 0
            })
        );
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(parse_booking(""), Err(ParseError::Empty));
        assert_eq!(parse_booking("   "), Err(ParseError::Empty));
        assert_eq!(parse_booking("A3"), Err(ParseError::MalformedSeparator));
        assert_eq!(parse_booking("A:3:4"), Err(ParseError::MalformedSeparator));
        assert_eq!(parse_booking(":3"), Err(ParseError::EmptySection));
        assert_eq!(parse_booking("A:"), Err(ParseError::InvalidQuantity));
        assert_eq!(parse_booking("A:x"), Err(ParseError::InvalidQuantity));
        assert_eq!(parse_booking("A:-1"), Err(ParseError::InvalidQuantity));
        // u32 overflow is a clean error, not a panic.
        assert_eq!(
            parse_booking("A:4294967296"),
            Err(ParseError::InvalidQuantity)
        );
    }

    proptest::proptest! {
        // Totality law (REQ-004): parse_booking never panics for ANY input.
        #[test]
        fn parse_is_total(input in ".*") {
            let _ = parse_booking(&input);
        }

        // Round-trip: a rendered well-formed booking parses back to itself. The
        // section alphabet is alphanumeric — non-empty, no ':' separator, and
        // invariant under trim (the parser strips surrounding whitespace, so a
        // whitespace-only/Unicode-space section is by design NOT well-formed).
        #[test]
        fn render_roundtrips(section in "[A-Za-z0-9]+", qty in any::<u32>()) {
            let rendered = format!("{section}:{qty}");
            proptest::prop_assert_eq!(
                parse_booking(&rendered),
                Ok(Booking { section, qty })
            );
        }
    }
}
