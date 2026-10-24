//! Environment-based configuration. All public URLs derive from base_url.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Outbound email (Resend). Present => password reset mails go out.
#[derive(Debug, Clone)]
pub struct MailConfig {
    pub resend_api_key: String,
    /// Verified sender, e.g. "Flash <no-reply@flash.example.com>".
    pub from: String,
}

/// An S3-compatible bucket for media blobs (AWS S3, Cloudflare R2, MinIO,
/// Backblaze B2, …). Absent => blobs live on local disk under
/// data_dir/media, which is what most installs want. All four must be set
/// together.
#[derive(Debug, Clone)]
pub struct MediaR2Config {
    /// Scheme + host of the S3 endpoint, e.g. https://s3.us-east-1.amazonaws.com
    pub endpoint: String,
    pub bucket: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Loopback by default: a reverse proxy or tunnel in front terminates TLS.
    pub bind: SocketAddr,
    /// FLASH_CLIENT_IP_HEADER: the request header that carries the real
    /// client address when a proxy sits in front (`cf-connecting-ip`,
    /// `x-real-ip`, …). Unset => the TCP peer address is used. Set it only
    /// when nothing but that proxy can reach the port, since any header
    /// is forgeable by a direct caller.
    pub client_ip_header: Option<String>,
    /// FLASH_ACCESS_LOG=1: one log line per request (method, route
    /// template, status, time, client address, browser), for diagnosing a
    /// client that fails somewhere between two requests. Off by default;
    /// an operator who turns it on also decides how long logs are kept.
    pub access_log: bool,
    /// Canonical public origin, e.g. https://cards.example.com
    pub base_url: String,
    /// FLASH_PREVIOUS_BASE_URLS: origins this server answered at before
    /// `base_url`, comma-separated. A connector registered back then
    /// still names `{origin}/mcp` as the OAuth resource, and it is this
    /// server all the same.
    pub previous_base_urls: Vec<String>,
    /// FLASH_SUPPORT_EMAIL: printed wherever a page shows a contact
    /// address. Unset hides those lines.
    pub support_email: Option<String>,
    pub data_dir: PathBuf,
    pub mail: Option<MailConfig>,
    /// FLASH_DEV_MAIL_LOG=1: a mailer without a provider. Every message
    /// and its link go to the log instead (dev / self-host convenience).
    pub dev_mail_log: bool,
    pub media_r2: Option<MediaR2Config>,
    /// FLASH_LANGUAGES: the languages readers may be given besides
    /// English, comma-separated ("es-ES,es-419"); unset gives the build's
    /// own (`i18n::SHIPPED`). English is always in. FLASH_PSEUDO_LOCALES=1
    /// adds the pseudo-languages (`Pseudo`), for testing a page in them.
    pub languages: Vec<flash_core::locale::UiLocale>,
}

/// FLASH_LANGUAGES and FLASH_PSEUDO_LOCALES as the server reads them: the
/// languages it gives readers, for a tool that must render what the
/// server renders (`i18n::server_catalog`).
pub fn languages_from_env() -> Result<Vec<flash_core::locale::UiLocale>, String> {
    languages_from(
        env_opt("FLASH_LANGUAGES"),
        std::env::var("FLASH_PSEUDO_LOCALES").is_ok_and(|v| v == "1"),
    )
}

/// FLASH_LANGUAGES, read: each tag one of the languages Flash's words
/// are written in; with `pseudo` (FLASH_PSEUDO_LOCALES=1), the
/// pseudo-languages besides, which FLASH_LANGUAGES never names.
fn languages_from(
    raw: Option<String>,
    pseudo: bool,
) -> Result<Vec<flash_core::locale::UiLocale>, String> {
    use flash_core::locale::UiLocale;
    let mut out = match raw {
        None => crate::i18n::SHIPPED.to_vec(),
        Some(raw) => {
            let mut out = vec![UiLocale::EN];
            for tag in raw.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                let locale = UiLocale::by_tag(tag)
                    .filter(|l| l.pseudo().is_none())
                    .ok_or_else(|| {
                        let known: Vec<&str> = UiLocale::real().map(UiLocale::tag).collect();
                        format!(
                            "FLASH_LANGUAGES: unknown language {tag:?} (expected one of {}; \
                             the pseudo-languages come with FLASH_PSEUDO_LOCALES=1)",
                            known.join(", ")
                        )
                    })?;
                if !out.contains(&locale) {
                    out.push(locale);
                }
            }
            out
        }
    };
    if pseudo {
        out.extend(UiLocale::all().filter(|l| l.pseudo().is_some()));
    }
    Ok(out)
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let bind = std::env::var("FLASH_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8437".to_string())
            .parse()
            .map_err(|e| format!("FLASH_BIND: {e}"))?;
        let base_url = std::env::var("FLASH_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8437".to_string())
            .trim_end_matches('/')
            .to_string();
        let previous_base_urls = env_list("FLASH_PREVIOUS_BASE_URLS")
            .into_iter()
            .map(|u| u.trim_end_matches('/').to_string())
            .collect();
        let support_email = env_opt("FLASH_SUPPORT_EMAIL");
        let data_dir =
            PathBuf::from(std::env::var("FLASH_DATA_DIR").unwrap_or_else(|_| "./data".to_string()));

        let mail = match (env_opt("RESEND_API_KEY"), env_opt("FLASH_EMAIL_FROM")) {
            (Some(resend_api_key), Some(from)) => Some(MailConfig {
                resend_api_key,
                from,
            }),
            (None, None) => None,
            _ => return Err("RESEND_API_KEY and FLASH_EMAIL_FROM must be set together".to_string()),
        };
        let dev_mail_log = std::env::var("FLASH_DEV_MAIL_LOG").is_ok_and(|v| v == "1");

        let r2_vars = [
            env_opt("FLASH_MEDIA_R2_ENDPOINT"),
            env_opt("FLASH_MEDIA_R2_BUCKET"),
            env_opt("FLASH_MEDIA_R2_ACCESS_KEY_ID"),
            env_opt("FLASH_MEDIA_R2_SECRET_ACCESS_KEY"),
        ];
        let media_r2 = match r2_vars {
            [Some(endpoint), Some(bucket), Some(access_key_id), Some(secret_access_key)] => {
                Some(MediaR2Config {
                    endpoint,
                    bucket,
                    access_key_id,
                    secret_access_key,
                })
            }
            [None, None, None, None] => None,
            _ => {
                return Err("FLASH_MEDIA_R2_ENDPOINT, FLASH_MEDIA_R2_BUCKET, \
                            FLASH_MEDIA_R2_ACCESS_KEY_ID, and FLASH_MEDIA_R2_SECRET_ACCESS_KEY \
                            must be set together"
                    .to_string())
            }
        };

        // Header names are case-insensitive on the wire; http::HeaderMap
        // wants them lowercase for lookup.
        let client_ip_header = env_opt("FLASH_CLIENT_IP_HEADER").map(|h| h.to_ascii_lowercase());
        let access_log = std::env::var("FLASH_ACCESS_LOG").is_ok_and(|v| v == "1");
        let languages = languages_from_env()?;

        Ok(Self {
            bind,
            client_ip_header,
            access_log,
            base_url,
            previous_base_urls,
            support_email,
            data_dir,
            mail,
            dev_mail_log,
            media_r2,
            languages,
        })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("flash.db")
    }
}

/// An env var, with blank treated as unset.
pub fn env_opt(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// A comma-separated list; unset or blank => empty.
pub fn env_list(name: &str) -> Vec<String> {
    env_opt(name)
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod languages_tests {
    use super::languages_from;
    use flash_core::locale::UiLocale;

    #[test]
    fn languages_are_read_from_the_variable() {
        assert_eq!(
            languages_from(None, false).unwrap(),
            crate::i18n::SHIPPED.to_vec()
        );
        let tag = |t| UiLocale::by_tag(t).unwrap();
        assert_eq!(
            languages_from(Some("es-ES, ES-419".into()), false).unwrap(),
            vec![UiLocale::EN, tag("es-ES"), tag("es-419")]
        );
        assert_eq!(
            languages_from(Some(String::new()), false).unwrap(),
            vec![UiLocale::EN]
        );
        assert!(languages_from(Some("fr".into()), false).is_err());
        // A pseudo-language comes only with its own switch.
        assert!(languages_from(Some("en-XA".into()), true).is_err());
        assert_eq!(
            languages_from(None, true).unwrap(),
            vec![UiLocale::EN, tag("en-XA"), tag("ar-XB")]
        );
    }
}
