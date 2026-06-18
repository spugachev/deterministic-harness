//! RESP command parser: untrusted bytes → a typed [`Command`].
//!
//! This is the client-facing front of the store. It supports a tiny subset —
//! `GET`, `SET`, `DEL` — encoded either as a RESP array of bulk strings (what a
//! real Redis client sends) or as a bare inline command line (handy for telnet /
//! tests). The contract that matters: **[`parse`] is a pure, total function that
//! NEVER panics on any input** — malformed, truncated, wrong arity, non-UTF-8,
//! or overlong. It returns a [`ParseError`] instead. That panic-freedom is what
//! the fuzz target asserts.
//!
//! No indexing, no slicing-by-range that could go out of bounds, no `unwrap` —
//! every byte access is via iterators / `split` so the parser cannot panic.

/// A parsed, typed client command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `GET <key>` — read the value at `key`.
    Get {
        /// The key to read.
        key: Vec<u8>,
    },
    /// `SET <key> <value>` — write `value` at `key`.
    Set {
        /// The key to write.
        key: Vec<u8>,
        /// The value to store.
        value: Vec<u8>,
    },
    /// `DEL <key>` — remove `key`.
    Del {
        /// The key to remove.
        key: Vec<u8>,
    },
}

/// Why a byte string failed to parse into a [`Command`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The input was empty or only whitespace.
    Empty,
    /// The RESP framing was malformed (bad prefix, length, or truncated).
    Malformed,
    /// The command verb is not one we support.
    UnknownCommand,
    /// The verb was recognised but the number of arguments was wrong.
    WrongArity,
    /// A length-prefixed bulk string exceeded the maximum we accept.
    TooLong,
}

/// The largest bulk string (in bytes) we will accept — guards against an
/// attacker-supplied length prefix demanding gigabytes.
pub const MAX_BULK_LEN: usize = 64 * 1024;

/// Parse a single command from `input`. Pure and total: returns `Ok(Command)`
/// or a [`ParseError`] for every possible input, and never panics.
///
/// # Errors
/// Returns a [`ParseError`] describing why the bytes are not a valid supported
/// command (empty, malformed framing, unknown verb, wrong arity, or overlong).
pub fn parse(input: &[u8]) -> Result<Command, ParseError> {
    // RESP arrays start with '*'; anything else we treat as an inline command.
    match input.first() {
        None => Err(ParseError::Empty),
        Some(b'*') => parse_array(input),
        Some(_) => parse_inline(input),
    }
}

/// Parse an inline command: whitespace-separated tokens on one line.
fn parse_inline(input: &[u8]) -> Result<Command, ParseError> {
    let tokens: Vec<&[u8]> = input
        .split(|&b| b == b' ' || b == b'\r' || b == b'\n' || b == b'\t')
        .filter(|t| !t.is_empty())
        .collect();
    command_from_tokens(&tokens)
}

/// Parse a RESP array of bulk strings, e.g. `*2\r\n$3\r\nGET\r\n$1\r\nk\r\n`.
fn parse_array(input: &[u8]) -> Result<Command, ParseError> {
    // Split on CRLF; each bulk string spans two logical lines ($len, payload).
    let mut lines = input.split(|&b| b == b'\n');

    // First line: `*<count>` (carriage returns are tolerated/stripped).
    let header = trim_cr(lines.next().ok_or(ParseError::Malformed)?);
    let count = parse_prefixed_len(header, b'*')?;

    let mut args: Vec<Vec<u8>> = Vec::new();
    for _ in 0..count {
        let len_line = trim_cr(lines.next().ok_or(ParseError::Malformed)?);
        let len = parse_prefixed_len(len_line, b'$')?;
        if len > MAX_BULK_LEN {
            return Err(ParseError::TooLong);
        }
        let payload = trim_cr(lines.next().ok_or(ParseError::Malformed)?);
        if payload.len() != len {
            return Err(ParseError::Malformed);
        }
        args.push(payload.to_vec());
    }

    let token_refs: Vec<&[u8]> = args.iter().map(Vec::as_slice).collect();
    command_from_tokens(&token_refs)
}

/// Strip a single trailing `\r` from a line (RESP terminator is `\r\n`).
fn trim_cr(line: &[u8]) -> &[u8] {
    match line.split_last() {
        Some((b'\r', rest)) => rest,
        _ => line,
    }
}

/// Parse a `<prefix><digits>` length header (e.g. `*2`, `$3`) into a `usize`.
/// Rejects a wrong/absent prefix, non-digits, and overlong numbers.
fn parse_prefixed_len(line: &[u8], prefix: u8) -> Result<usize, ParseError> {
    match line.split_first() {
        Some((&first, digits)) if first == prefix => parse_usize(digits),
        _ => Err(ParseError::Malformed),
    }
}

/// Parse ASCII digits into a `usize` without panicking on overflow or non-digits.
fn parse_usize(digits: &[u8]) -> Result<usize, ParseError> {
    if digits.is_empty() {
        return Err(ParseError::Malformed);
    }
    let mut acc: usize = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return Err(ParseError::Malformed);
        }
        // Checked arithmetic: a colossal length prefix becomes TooLong, never a
        // panic or wraparound.
        let digit = usize::from(b - b'0');
        acc = acc
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(ParseError::TooLong)?;
    }
    Ok(acc)
}

/// Build a [`Command`] from already-tokenised argv, checking verb + arity.
fn command_from_tokens(tokens: &[&[u8]]) -> Result<Command, ParseError> {
    let (verb, rest) = tokens.split_first().ok_or(ParseError::Empty)?;
    if verb_eq_ignore_case(verb, b"GET") {
        match rest {
            [key] => Ok(Command::Get {
                key: (*key).to_vec(),
            }),
            _ => Err(ParseError::WrongArity),
        }
    } else if verb_eq_ignore_case(verb, b"SET") {
        match rest {
            [key, value] => Ok(Command::Set {
                key: (*key).to_vec(),
                value: (*value).to_vec(),
            }),
            _ => Err(ParseError::WrongArity),
        }
    } else if verb_eq_ignore_case(verb, b"DEL") {
        match rest {
            [key] => Ok(Command::Del {
                key: (*key).to_vec(),
            }),
            _ => Err(ParseError::WrongArity),
        }
    } else {
        Err(ParseError::UnknownCommand)
    }
}

/// Case-insensitive ASCII comparison of a verb token against a literal.
fn verb_eq_ignore_case(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.eq_ignore_ascii_case(y))
}

#[cfg(test)]
mod tests {
    use super::{parse, Command, ParseError, MAX_BULK_LEN};

    #[test]
    fn parses_inline_get() {
        assert_eq!(
            parse(b"GET foo"),
            Ok(Command::Get {
                key: b"foo".to_vec()
            })
        );
    }

    #[test]
    fn parses_inline_set_and_del() {
        assert_eq!(
            parse(b"SET k v"),
            Ok(Command::Set {
                key: b"k".to_vec(),
                value: b"v".to_vec()
            })
        );
        assert_eq!(parse(b"DEL k"), Ok(Command::Del { key: b"k".to_vec() }));
    }

    #[test]
    fn verb_is_case_insensitive() {
        assert_eq!(
            parse(b"get foo"),
            Ok(Command::Get {
                key: b"foo".to_vec()
            })
        );
        assert!(parse(b"Set k v").is_ok());
    }

    #[test]
    fn parses_resp_array_get() {
        assert_eq!(
            parse(b"*2\r\n$3\r\nGET\r\n$3\r\nfoo\r\n"),
            Ok(Command::Get {
                key: b"foo".to_vec()
            })
        );
    }

    #[test]
    fn parses_resp_array_set() {
        assert_eq!(
            parse(b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$3\r\nval\r\n"),
            Ok(Command::Set {
                key: b"k".to_vec(),
                value: b"val".to_vec()
            })
        );
    }

    #[test]
    fn empty_input_is_empty_error() {
        assert_eq!(parse(b""), Err(ParseError::Empty));
        assert_eq!(parse(b"   "), Err(ParseError::Empty));
    }

    #[test]
    fn unknown_verb_rejected() {
        assert_eq!(parse(b"INCR foo"), Err(ParseError::UnknownCommand));
    }

    #[test]
    fn wrong_arity_rejected() {
        assert_eq!(parse(b"GET"), Err(ParseError::WrongArity));
        assert_eq!(parse(b"GET a b"), Err(ParseError::WrongArity));
        assert_eq!(parse(b"SET k"), Err(ParseError::WrongArity));
    }

    #[test]
    fn truncated_array_is_malformed() {
        assert_eq!(parse(b"*2\r\n$3\r\nGET\r\n"), Err(ParseError::Malformed));
    }

    #[test]
    fn bad_length_prefix_is_malformed() {
        assert_eq!(parse(b"*x\r\n"), Err(ParseError::Malformed));
        // Declared length does not match payload.
        assert_eq!(parse(b"*1\r\n$9\r\nGET\r\n"), Err(ParseError::Malformed));
    }

    #[test]
    fn overlong_length_is_too_long() {
        let huge = format!("*1\r\n${}\r\n", MAX_BULK_LEN + 1);
        assert_eq!(parse(huge.as_bytes()), Err(ParseError::TooLong));
        // A length prefix that would overflow usize → TooLong, not a panic.
        assert_eq!(
            parse(b"*1\r\n$999999999999999999999999\r\n"),
            Err(ParseError::TooLong)
        );
    }

    #[test]
    fn non_utf8_key_is_accepted_as_bytes() {
        // Keys are bytes, not UTF-8 — a 0xFF byte key parses fine.
        assert_eq!(
            parse(&[b'G', b'E', b'T', b' ', 0xFF, 0xFE]),
            Ok(Command::Get {
                key: vec![0xFF, 0xFE]
            })
        );
    }

    proptest::proptest! {
        // The headline law: parse NEVER panics, for ANY input bytes.
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..512)) {
            let _ = parse(&bytes);
        }

        // Round-trip: a well-formed inline command parses to the expected variant.
        #[test]
        fn inline_set_roundtrips(
            k in "[a-z]{1,8}",
            v in "[a-z]{1,8}",
        ) {
            let line = format!("SET {k} {v}");
            proptest::prop_assert_eq!(
                parse(line.as_bytes()),
                Ok(Command::Set { key: k.into_bytes(), value: v.into_bytes() })
            );
        }
    }
}
