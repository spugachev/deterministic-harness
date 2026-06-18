//! RESP command parser: untrusted bytes -> typed command.
//!
//! `parse` is a pure, total function: it NEVER panics on any input (malformed,
//! truncated, wrong arity, non-UTF8, overlong). All failure modes are returned
//! as `Err(ParseError)`.

/// The small command subset this KV store understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Get { key: String },
    Set { key: String, value: String },
    Del { key: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Input did not contain a complete RESP frame yet.
    Incomplete,
    /// The bytes were structurally invalid RESP.
    Malformed(&'static str),
    /// A command name we do not support, or wrong number of arguments.
    UnknownCommand,
    WrongArity,
    /// An argument was not valid UTF-8.
    NonUtf8,
}

/// Maximum number of array elements / bulk-string length we will accept.
/// Guards against overlong / hostile inputs declaring huge sizes.
const MAX_ELEMENTS: i64 = 1024;
const MAX_BULK_LEN: i64 = 64 * 1024;

/// Parse a single RESP command from `input`.
///
/// Accepts the canonical RESP array-of-bulk-strings form, e.g.
/// `*3\r\n$3\r\nSET\r\n$3\r\nfoo\r\n$3\r\nbar\r\n`. Also accepts the simpler
/// inline form (space-separated tokens terminated by `\r\n` or `\n`) so the
/// store is easy to drive from a console.
pub fn parse(input: &[u8]) -> Result<Command, ParseError> {
    match input.first() {
        None => Err(ParseError::Incomplete),
        Some(b'*') => parse_array(input),
        Some(_) => parse_inline(input),
    }
}

/// RESP array of bulk strings.
fn parse_array(input: &[u8]) -> Result<Command, ParseError> {
    let mut pos = 0usize;
    let count = read_prefixed_len(input, &mut pos, b'*')?;
    if count <= 0 {
        return Err(ParseError::Malformed("empty array"));
    }
    if count > MAX_ELEMENTS {
        return Err(ParseError::Malformed("too many elements"));
    }

    let mut args: Vec<String> = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let len = read_prefixed_len(input, &mut pos, b'$')?;
        if len < 0 {
            return Err(ParseError::Malformed("null bulk string"));
        }
        if len > MAX_BULK_LEN {
            return Err(ParseError::Malformed("bulk string too long"));
        }
        let len = len as usize;
        // Need `len` bytes of payload followed by CRLF.
        if pos + len + 2 > input.len() {
            return Err(ParseError::Incomplete);
        }
        let payload = &input[pos..pos + len];
        pos += len;
        if input.get(pos) != Some(&b'\r') || input.get(pos + 1) != Some(&b'\n') {
            return Err(ParseError::Malformed("bulk string not CRLF-terminated"));
        }
        pos += 2;
        let s = std::str::from_utf8(payload).map_err(|_| ParseError::NonUtf8)?;
        args.push(s.to_string());
    }

    build_command(args)
}

/// Read a `<prefix><number>\r\n` header, advancing `pos` past it.
fn read_prefixed_len(
    input: &[u8],
    pos: &mut usize,
    prefix: u8,
) -> Result<i64, ParseError> {
    if input.get(*pos) != Some(&prefix) {
        return Err(ParseError::Malformed("expected length prefix"));
    }
    *pos += 1;
    let line = read_line(input, pos)?;
    parse_int(line)
}

/// Read up to (and consuming) the next CRLF, returning the bytes before it.
fn read_line<'a>(input: &'a [u8], pos: &mut usize) -> Result<&'a [u8], ParseError> {
    let start = *pos;
    let mut i = start;
    while i + 1 < input.len() {
        if input[i] == b'\r' && input[i + 1] == b'\n' {
            let line = &input[start..i];
            *pos = i + 2;
            return Ok(line);
        }
        i += 1;
    }
    Err(ParseError::Incomplete)
}

fn parse_int(bytes: &[u8]) -> Result<i64, ParseError> {
    if bytes.is_empty() {
        return Err(ParseError::Malformed("empty integer"));
    }
    let s = std::str::from_utf8(bytes).map_err(|_| ParseError::Malformed("non-utf8 integer"))?;
    s.parse::<i64>()
        .map_err(|_| ParseError::Malformed("invalid integer"))
}

/// Inline form: split on ASCII whitespace, terminated by a newline.
fn parse_inline(input: &[u8]) -> Result<Command, ParseError> {
    // Require a line terminator so a truncated inline command is `Incomplete`.
    let mut pos = 0usize;
    let line = match read_line(input, &mut pos) {
        Ok(l) => l,
        Err(_) => {
            // Tolerate a trailing bare `\n` with no `\r`.
            if let Some(idx) = input.iter().position(|&b| b == b'\n') {
                &input[..idx]
            } else {
                return Err(ParseError::Incomplete);
            }
        }
    };
    let text = std::str::from_utf8(line).map_err(|_| ParseError::NonUtf8)?;
    let args: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();
    if args.is_empty() {
        return Err(ParseError::Malformed("empty command"));
    }
    build_command(args)
}

fn build_command(args: Vec<String>) -> Result<Command, ParseError> {
    let name = args[0].to_ascii_uppercase();
    match name.as_str() {
        "GET" => {
            if args.len() != 2 {
                return Err(ParseError::WrongArity);
            }
            Ok(Command::Get {
                key: args[1].clone(),
            })
        }
        "SET" => {
            if args.len() != 3 {
                return Err(ParseError::WrongArity);
            }
            Ok(Command::Set {
                key: args[1].clone(),
                value: args[2].clone(),
            })
        }
        "DEL" => {
            if args.len() != 2 {
                return Err(ParseError::WrongArity);
            }
            Ok(Command::Del {
                key: args[1].clone(),
            })
        }
        _ => Err(ParseError::UnknownCommand),
    }
}
