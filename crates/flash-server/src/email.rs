//! Outbound email: the password-reset loop here, and any mail an
//! extension sends. One trait, three implementations: Resend, journal
//! logging (`FLASH_DEV_MAIL_LOG`), and capture (tests).
//! Sync by design: senders run inside spawn_blocking like every other
//! blocking call in this codebase.

use crate::config::MailConfig;
use crate::i18n::filters;
use crate::state::Site;

#[derive(Debug, Clone, Default)]
pub struct OutgoingEmail {
    pub to: String,
    pub subject: crate::i18n::Words,
    pub text: String,
    pub html: String,
    /// Where a reply to this one message should go, when that is not
    /// the deployment's support address: a message that forwards what
    /// somebody wrote wants Reply to reach them, not us. `None` leaves
    /// the transport's own address in place.
    pub reply_to: Option<String>,
    /// Extra headers for this one message, as (name, value) pairs: a
    /// mailing a person can switch off carries its unsubscribe headers
    /// here. Empty for ordinary account mail.
    pub headers: Vec<(String, String)>,
    /// The sender for this one message, when it is not the transport's
    /// own. `None` keeps the configured From.
    pub from: Option<String>,
    /// Names this one send, so a retry of the same send (a job run again
    /// after a crash, or after the provider timed out mid-reply) is not a
    /// second mail: the provider answers the repeat without sending it.
    /// `None` for mail whose loss a person notices and retries themselves.
    pub idempotency_key: Option<String>,
}

pub trait Mailer: Send + Sync {
    fn send(&self, mail: &OutgoingEmail) -> Result<(), String>;

    /// `send`, answering with the provider's id for the message when it
    /// gives one.
    fn send_tracked(&self, mail: &OutgoingEmail) -> Result<Option<String>, String> {
        self.send(mail).map(|()| None)
    }
}

pub struct ResendMailer {
    api_key: String,
    from: String,
    reply_to: Option<String>,
    agent: ureq::Agent,
}

impl ResendMailer {
    /// `reply_to` is the deployment's support address: every message this
    /// sends is transactional and wants the same one, so it belongs on the
    /// transport beside `from` rather than on each message. Unset means the
    /// header is omitted entirely, and a reply goes to the sending address.
    pub fn new(config: &MailConfig, reply_to: Option<&str>) -> Self {
        Self {
            api_key: config.resend_api_key.clone(),
            from: config.from.clone(),
            reply_to: reply_to.map(str::to_string),
            // Errors come back as responses, so a repeated key can be read.
            agent: crate::http::agent(std::time::Duration::from_secs(15), false),
        }
    }
}

impl Mailer for ResendMailer {
    fn send(&self, mail: &OutgoingEmail) -> Result<(), String> {
        self.send_tracked(mail).map(|_| ())
    }

    /// The id comes back in the provider's answer; a retry it answers
    /// "already sent" has none, and the first try's id stands.
    fn send_tracked(&self, mail: &OutgoingEmail) -> Result<Option<String>, String> {
        let payload = resend_payload(&self.from, self.reply_to.as_deref(), mail);
        let mut request = self
            .agent
            .post("https://api.resend.com/emails")
            .header("Authorization", &format!("Bearer {}", self.api_key));
        if let Some(key) = &mail.idempotency_key {
            request = request.header("Idempotency-Key", key);
        }
        let mut response = request
            .send_json(payload)
            .map_err(|e| format!("resend: {e}"))?;
        let status = response.status();
        let body = response.body_mut().read_to_string().unwrap_or_default();
        if status.is_success() {
            return Ok(message_id(&body));
        }
        if status.as_u16() == 409 && key_already_sent(&body) {
            return Ok(None);
        }
        Err(format!("resend: HTTP {status}"))
    }
}

/// The message id in the provider's answer to a send (`{"id": "..."}`),
/// when it is a plausible one.
fn message_id(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("id")?
        .as_str()
        .filter(|id| {
            (1..=100).contains(&id.len())
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        .map(str::to_string)
}

/// Whether a 409 says this message's key already sent a mail. A retry
/// rebuilds its mail, and a fresh sign-in link makes the body differ, so
/// the provider answers a repeat of a send that went through with a
/// conflict rather than the original reply. A send still in flight under
/// the same key is a different conflict, worth trying again.
fn key_already_sent(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("name")?
                .as_str()
                .map(|n| n == "invalid_idempotent_request")
        })
        .unwrap_or(false)
}

/// The body Resend's send endpoint takes for one message. `from` and
/// `reply_to` are the transport's; a message's own override them.
fn resend_payload(from: &str, reply_to: Option<&str>, mail: &OutgoingEmail) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "from": mail.from.as_deref().unwrap_or(from),
        "to": [mail.to],
        "subject": mail.subject,
        "text": mail.text,
        "html": mail.html,
    });
    // Absent, not null: an explicit null is a value the API would have
    // to interpret, where a missing key is simply no header. A message
    // that names its own address overrides the transport's, which is
    // how a forwarded report gets Reply to reach whoever wrote it.
    if let Some(reply_to) = mail.reply_to.as_deref().or(reply_to) {
        payload["reply_to"] = serde_json::json!([reply_to]);
    }
    if !mail.headers.is_empty() {
        let headers: serde_json::Map<String, serde_json::Value> = mail
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), serde_json::Value::String(value.clone())))
            .collect();
        payload["headers"] = serde_json::Value::Object(headers);
    }
    payload
}

// ---- dev / self-host: the link lands in the journal ----
// Mirrors the first-boot bootstrap invite, which is also journal-delivered.

pub struct LogMailer;

impl Mailer for LogMailer {
    fn send(&self, mail: &OutgoingEmail) -> Result<(), String> {
        tracing::info!("dev mail to {}: {}", mail.to, mail.subject);
        if let Some(reply_to) = &mail.reply_to {
            tracing::info!("dev mail reply-to: {reply_to}");
        }
        if let Some(from) = &mail.from {
            tracing::info!("dev mail from: {from}");
        }
        for (name, value) in &mail.headers {
            tracing::info!("dev mail header {name:?}: {value:?}");
        }
        tracing::info!("{}", mail.text);
        Ok(())
    }
}

// ---- tests: capture instead of sending ----

#[derive(Default)]
pub struct CaptureMailer(pub parking_lot::Mutex<Vec<OutgoingEmail>>);

impl Mailer for CaptureMailer {
    fn send(&self, mail: &OutgoingEmail) -> Result<(), String> {
        self.0.lock().push(mail.clone());
        Ok(())
    }
}

// ---- rendering ----

/// Renders a mail body in the language of the work at hand (the
/// recipient's, set with `i18n::scoped`). A template failure falls back
/// to the plain-text part rather than propagating: a reset link is
/// someone's way back into their account, and losing one to a formatting
/// error is worse than sending a plain-looking message.
pub fn render_html<T: askama::Template>(template: &T, text: &str) -> String {
    crate::i18n::render_current(template).unwrap_or_else(|e| {
        tracing::error!("email template: {e}");
        format!("<pre>{}</pre>", html_escape(text))
    })
}

/// The sign-off the plain-text part carries, mirroring the HTML footer.
pub fn text_footer(site: &Site) -> String {
    match &site.support_email {
        Some(addr) => format!(
            "\n--\nFlash · {}\n{}\n",
            site.host(),
            crate::i18n::ta("email-questions-text", &[("email", addr.as_str().into())])
        ),
        None => format!("\n--\nFlash · {}\n", site.host()),
    }
}

// ---- message builders ----

#[derive(askama::Template)]
#[template(path = "email_password_reset.html")]
struct PasswordResetHtml<'a> {
    site: &'a Site,
    link: &'a str,
}

/// The reset email, in the language of the work at hand (the account's,
/// set by the caller with `i18n::scoped`).
pub fn password_reset_email(site: &Site, token: &str) -> OutgoingEmail {
    use crate::i18n::t;
    let link = crate::flows::reset::link(&site.base_url, token);
    let text = format!(
        "{}\n\n{}\n\n{link}\n\n{}\n{}",
        t("email-reset-body"),
        t("email-reset-here"),
        t("email-reset-text-end"),
        text_footer(site)
    );
    OutgoingEmail {
        to: String::new(), // caller fills in
        subject: t("email-reset-subject"),
        html: render_html(&PasswordResetHtml { site, link: &link }, &text),
        text,
        ..Default::default()
    }
}

/// The five HTML-significant characters, for text dropped into a mail body.
/// `'` is numeric (`&apos;` is undefined in HTML4 and some mail renderers
/// print it literally), and `&` goes first so the entities it writes are
/// not escaped again.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_used_key_counts_as_sent() {
        assert!(key_already_sent(
            r#"{"statusCode":409,"name":"invalid_idempotent_request","message":"x"}"#
        ));
        assert!(!key_already_sent(
            r#"{"statusCode":409,"name":"concurrent_idempotent_requests","message":"x"}"#
        ));
        assert!(!key_already_sent("not json"));
    }

    /// A deployment that names no host: the fixture every mail test builds on.
    fn site(support_email: Option<&str>) -> Site {
        Site {
            base_url: "https://flash.example.com".to_string(),
            support_email: support_email.map(str::to_string),
            i18n: crate::i18n::core_catalog(),
        }
    }

    #[test]
    fn password_reset_email_carries_the_reset_link() {
        let mail = password_reset_email(&site(None), "tok456");
        assert!(mail.text.contains("https://flash.example.com/reset/tok456"));
        assert!(mail.html.contains("https://flash.example.com/reset/tok456"));
    }

    #[test]
    fn plain_mail_sends_from_the_transport_with_no_extra_headers() {
        let mail = OutgoingEmail {
            to: "a@example.com".to_string(),
            subject: crate::i18n::Words::test("Hi"),
            ..Default::default()
        };
        let payload = resend_payload(
            "Flash <no-reply@example.com>",
            Some("help@example.com"),
            &mail,
        );
        assert_eq!(payload["from"], "Flash <no-reply@example.com>");
        assert_eq!(payload["reply_to"], serde_json::json!(["help@example.com"]));
        assert!(payload.get("headers").is_none(), "{payload}");
        let bare = resend_payload("Flash <no-reply@example.com>", None, &mail);
        assert!(bare.get("reply_to").is_none(), "{bare}");
    }

    #[test]
    fn a_message_can_name_its_own_sender_and_headers() {
        let mail = OutgoingEmail {
            to: "a@example.com".to_string(),
            from: Some("Flash <study@mail.example.com>".to_string()),
            headers: vec![
                (
                    "List-Unsubscribe".to_string(),
                    "<https://example.com/u/1.x>".to_string(),
                ),
                (
                    "List-Unsubscribe-Post".to_string(),
                    "List-Unsubscribe=One-Click".to_string(),
                ),
            ],
            ..Default::default()
        };
        let payload = resend_payload("Flash <no-reply@example.com>", None, &mail);
        assert_eq!(payload["from"], "Flash <study@mail.example.com>");
        assert_eq!(
            payload["headers"],
            serde_json::json!({
                "List-Unsubscribe": "<https://example.com/u/1.x>",
                "List-Unsubscribe-Post": "List-Unsubscribe=One-Click",
            })
        );
    }

    #[test]
    fn a_sends_answer_gives_its_message_id() {
        assert_eq!(
            message_id(r#"{"id":"49a3999c-0ce1-4ea6-ab68-afcd6dc2e794"}"#).as_deref(),
            Some("49a3999c-0ce1-4ea6-ab68-afcd6dc2e794")
        );
        assert_eq!(message_id(r#"{"id":"not an id <script>"}"#), None);
        assert_eq!(message_id("{}"), None);
        assert_eq!(message_id("not json"), None);
    }

    #[test]
    fn html_escape_covers_all_five() {
        assert_eq!(
            html_escape("it's \"x\" <b>&</b>"),
            "it&#39;s &quot;x&quot; &lt;b&gt;&amp;&lt;/b&gt;"
        );
    }
}
