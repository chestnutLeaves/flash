//! The one outbound HTTPS client configuration. Every external call the
//! server makes (Resend, the media bucket, a captcha verifier, and whatever
//! an extension adds) builds its agent here so TLS provider and timeouts
//! can never drift between them.

use std::time::Duration;

/// Native TLS on purpose: the vendored OpenSSL is already linked
/// (webauthn-rs) and ureq 3 would otherwise pull in rustls as well.
/// The roots are the platform's own, not ureq's bundled Mozilla set:
/// handing native TLS a specific root list makes Windows' verifier
/// insist the chain end in one of them, and it refuses Google's with
/// "unable to find any user-specified roots". Connecting gets 5 s
/// everywhere; `global_timeout` bounds the whole exchange.
/// `status_as_error = false` hands 4xx/5xx replies back as responses so
/// callers can read the JSON error body.
pub fn agent(global_timeout: Duration, status_as_error: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(global_timeout))
        .http_status_as_error(status_as_error)
        .build()
        .into()
}
