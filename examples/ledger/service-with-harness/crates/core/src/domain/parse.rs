//! The untrusted single-line transfer-command parser.
//!
//! `TRANSFER <from_id> <to_id> <amount_cents> <idempotency_key>` —
//! whitespace-separated; ids and amount are unsigned integers; the key is an
//! opaque token. [`parse_transfer`] takes ARBITRARY bytes and must return a
//! typed result or a typed error — it NEVER panics (malformed, overlong, empty,
//! non-numeric, overflowing, missing/extra fields). This is the fuzz target.

/// A parsed, well-formed transfer command. The key is owned so the command can
/// outlive the input buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferCommand {
    /// Source account id.
    pub from: u64,
    /// Destination account id.
    pub to: u64,
    /// Amount to move, in integer cents.
    pub amount_cents: u64,
    /// Opaque idempotency token.
    pub key: String,
}

/// Why a line failed to parse. Typed so callers can branch without string
/// matching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// Input was not valid UTF-8.
    NotUtf8,
    /// The leading `TRANSFER` verb was missing or misspelled.
    BadVerb,
    /// Wrong number of whitespace-separated fields (need exactly 5).
    WrongFieldCount,
    /// An id or amount field was not a base-10 unsigned integer (or overflowed).
    BadNumber,
    /// The idempotency key was empty.
    EmptyKey,
}

/// Parse arbitrary bytes into a [`TransferCommand`]. Total and panic-free for
/// every input.
///
/// # Errors
/// Returns a [`ParseError`] describing the first problem encountered.
pub fn parse_transfer(input: &[u8]) -> Result<TransferCommand, ParseError> {
    let text = core::str::from_utf8(input).map_err(|_| ParseError::NotUtf8)?;
    parse_transfer_str(text)
}

/// Parse a `str` into a [`TransferCommand`]. Total and panic-free.
///
/// # Errors
/// Returns a [`ParseError`] describing the first problem encountered.
pub fn parse_transfer_str(text: &str) -> Result<TransferCommand, ParseError> {
    // `split_whitespace` collapses arbitrary runs of any Unicode whitespace and
    // ignores leading/trailing space — robust against ragged input.
    let mut fields = text.split_whitespace();

    let verb = fields.next().ok_or(ParseError::WrongFieldCount)?;
    if verb != "TRANSFER" {
        return Err(ParseError::BadVerb);
    }

    let from = parse_u64(fields.next().ok_or(ParseError::WrongFieldCount)?)?;
    let to = parse_u64(fields.next().ok_or(ParseError::WrongFieldCount)?)?;
    let amount_cents = parse_u64(fields.next().ok_or(ParseError::WrongFieldCount)?)?;
    let key = fields.next().ok_or(ParseError::WrongFieldCount)?;

    // Exactly five fields: any trailing token is an error (extra fields).
    if fields.next().is_some() {
        return Err(ParseError::WrongFieldCount);
    }
    if key.is_empty() {
        return Err(ParseError::EmptyKey);
    }

    Ok(TransferCommand {
        from,
        to,
        amount_cents,
        key: key.to_owned(),
    })
}

/// Parse a strictly base-10 unsigned integer. `str::parse` rejects signs,
/// non-digits, empties, and overflow — so the parser never panics on those.
fn parse_u64(field: &str) -> Result<u64, ParseError> {
    field.parse::<u64>().map_err(|_| ParseError::BadNumber)
}

#[cfg(test)]
mod tests {
    use super::{parse_transfer, parse_transfer_str, ParseError, TransferCommand};

    #[test]
    fn parses_a_well_formed_line() {
        assert_eq!(
            parse_transfer_str("TRANSFER 1 2 500 abc-key"),
            Ok(TransferCommand {
                from: 1,
                to: 2,
                amount_cents: 500,
                key: "abc-key".to_owned(),
            })
        );
    }

    #[test]
    fn tolerates_extra_whitespace() {
        assert_eq!(
            parse_transfer_str("  TRANSFER\t3   4  10\tk  "),
            Ok(TransferCommand {
                from: 3,
                to: 4,
                amount_cents: 10,
                key: "k".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_malformed_input_without_panicking() {
        assert_eq!(parse_transfer_str(""), Err(ParseError::WrongFieldCount));
        assert_eq!(parse_transfer_str("NOPE 1 2 3 k"), Err(ParseError::BadVerb));
        assert_eq!(
            parse_transfer_str("TRANSFER 1 2 3"),
            Err(ParseError::WrongFieldCount)
        );
        assert_eq!(
            parse_transfer_str("TRANSFER 1 2 3 k extra"),
            Err(ParseError::WrongFieldCount)
        );
        assert_eq!(
            parse_transfer_str("TRANSFER x 2 3 k"),
            Err(ParseError::BadNumber)
        );
        assert_eq!(
            parse_transfer_str("TRANSFER -1 2 3 k"),
            Err(ParseError::BadNumber)
        );
        // u64::MAX + 1 overflows → BadNumber, not a panic.
        assert_eq!(
            parse_transfer_str("TRANSFER 18446744073709551616 2 3 k"),
            Err(ParseError::BadNumber)
        );
    }

    #[test]
    fn rejects_non_utf8_bytes() {
        assert_eq!(parse_transfer(&[0xff, 0xfe]), Err(ParseError::NotUtf8));
    }

    proptest::proptest! {
        // The core safety property: parsing ARBITRARY bytes never panics; it
        // always returns a typed Ok/Err. (The fuzz target proves this over a far
        // wider corpus; this is the cheap always-on version.)
        #[test]
        fn never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..64)) {
            let _ = parse_transfer(&bytes);
        }

        // Round-trip: any well-formed command re-parses to itself.
        #[test]
        fn roundtrips_well_formed(from in 0_u64.., to in 0_u64.., amount in 0_u64.., key in "[A-Za-z0-9_-]{1,16}") {
            let line = format!("TRANSFER {from} {to} {amount} {key}");
            let parsed = parse_transfer_str(&line).expect("well-formed line parses");
            proptest::prop_assert_eq!(parsed.from, from);
            proptest::prop_assert_eq!(parsed.to, to);
            proptest::prop_assert_eq!(parsed.amount_cents, amount);
            proptest::prop_assert_eq!(parsed.key, key);
        }
    }
}
