//! The one outbound HTTP client configuration. Every external call the
//! server makes over ureq (Resend, the media bucket, a captcha verifier,
//! and an extension's own) gets its agent here, so the TLS provider, the
//! trust decision and the timeouts can never drift between them.

use std::time::Duration;

/// rustls on ring, with the platform's own verifier and roots rather than
/// ureq's bundled Mozilla set, so a host's trust store (and, on Windows,
/// its certificate policy) decides what a server may present, as it did
/// under native TLS. Connecting gets 5 s everywhere; `global_timeout`
/// bounds the whole exchange.
/// `status_as_error = false` hands 4xx/5xx replies back as responses so
/// callers can read the JSON error body.
pub fn agent(global_timeout: Duration, status_as_error: bool) -> ureq::Agent {
    config(global_timeout, status_as_error).into()
}

/// `agent`'s settings, for an agent the test harness builds on an
/// in-memory transport (`testing::MemoryHost`): the same timeouts and
/// status handling as production's, so the two cannot drift.
pub(crate) fn config(global_timeout: Duration, status_as_error: bool) -> ureq::config::Config {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::Rustls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(global_timeout))
        .http_status_as_error(status_as_error)
        .build()
}
