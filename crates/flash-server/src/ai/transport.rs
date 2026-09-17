//! What every HTTP adapter shares of the wire: how a client error
//! becomes an `LlmError`, and how a call's time is measured. Nothing
//! here knows a provider.

use std::time::Instant;

use super::provider::LlmError;

/// A failure before any status came back: a timeout, a refused
/// connection, a broken exchange. A status the client turned into an
/// error keeps its number; anything else is status 0, "could not be
/// reached".
pub(super) fn transport_error(e: ureq::Error) -> LlmError {
    match e {
        ureq::Error::Timeout(_) => LlmError::Timeout,
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => LlmError::Timeout,
        ureq::Error::StatusCode(code) => LlmError::http(code, ""),
        other => LlmError::http(0, &other.to_string()),
    }
}

/// Milliseconds since `since`, saturated to what the ledger stores.
pub(super) fn elapsed_ms(since: Instant) -> u32 {
    since.elapsed().as_millis().min(u32::MAX as u128) as u32
}
