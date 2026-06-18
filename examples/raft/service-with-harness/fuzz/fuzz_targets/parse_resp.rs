//! Fuzz target: the RESP command parser must NEVER panic on any input.
//!
//! `core::domain::resp::parse` is the project's only untrusted-byte surface and
//! is contracted to be pure and total. This target feeds it arbitrary bytes; the
//! fuzzer's job is to find any input that panics, overflows, or hangs. There is
//! no oracle beyond "does not crash" — that panic-freedom IS the property.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Must return Ok/Err for every input, never panic.
    let _ = core::domain::resp::parse(data);
});
