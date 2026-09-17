//! Outbound email: the password-reset loop here, the hosted product's
//! signup loop on top. One trait, three implementations: Resend
//! (production), journal logging (dev/self-host), and capture (tests).
//! Sync by design: senders run inside spawn_blocking like every other
//! blocking call in this codebase.

use crate::config::MailConfig;
use crate::state::Site;

#[derive(Debug, Clone)]
pub struct OutgoingEmail {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

pub trait Mailer: Send + Sync {
    fn send(&self, mail: &OutgoingEmail) -> Result<(), String>;
}

// ---- production: Resend HTTPS API ----

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
            agent: crate::http::agent(std::time::Duration::from_secs(15), true),
        }
    }
}

impl Mailer for ResendMailer {
    fn send(&self, mail: &OutgoingEmail) -> Result<(), String> {
        let mut payload = serde_json::json!({
            "from": self.from,
            "to": [mail.to],
            "subject": mail.subject,
            "text": mail.text,
            "html": mail.html,
        });
        // Absent, not null: an explicit null is a value the API would have
        // to interpret, where a missing key is simply no header.
        if let Some(reply_to) = &self.reply_to {
            payload["reply_to"] = serde_json::json!([reply_to]);
        }
        let response = self
            .agent
            .post("https://api.resend.com/emails")
            .header("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(payload)
            .map_err(|e| format!("resend: {e}"))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("resend: HTTP {}", response.status()))
        }
    }
}

// ---- dev / self-host: the link lands in the journal ----
// Mirrors the first-boot bootstrap invite, which is also journal-delivered.

pub struct LogMailer;

impl Mailer for LogMailer {
    fn send(&self, mail: &OutgoingEmail) -> Result<(), String> {
        tracing::info!("dev mail to {}: {}", mail.to, mail.subject);
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

/// Renders a mail body. A template failure falls back to the plain-text
/// part rather than propagating: a reset link is someone's way back into
/// their account, and losing one to a formatting error is worse than
/// sending a plain-looking message.
pub fn render_html<T: askama::Template>(template: &T, text: &str) -> String {
    template.render().unwrap_or_else(|e| {
        tracing::error!("email template: {e}");
        format!("<pre>{}</pre>", html_escape(text))
    })
}

/// The sign-off the plain-text part carries, mirroring the HTML footer.
pub fn text_footer(site: &Site) -> String {
    match &site.support_email {
        Some(addr) => format!(
            "\n--\nFlash · {}\nQuestions? Reply to this email, or write to {addr}\n",
            site.host()
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

pub fn password_reset_email(site: &Site, token: &str) -> OutgoingEmail {
    let link = format!("{}/reset/{token}", site.base_url);
    let text = format!(
        "Someone (probably you) asked to reset the password for this Flash account.\n\n\
         Set a new password here:\n\n{link}\n\nThe link works once and expires in \
         1 hour. If this wasn't you, ignore this email. Nothing changes.\n{}",
        text_footer(site)
    );
    OutgoingEmail {
        to: String::new(), // caller fills in
        subject: "Reset your Flash password".to_string(),
        html: render_html(&PasswordResetHtml { site, link: &link }, &text),
        text,
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

    /// A deployment that names no host: the fixture every mail test builds on.
    fn site(support_email: Option<&str>) -> Site {
        Site {
            base_url: "https://flash.example.com".to_string(),
            support_email: support_email.map(str::to_string),
        }
    }

    #[test]
    fn password_reset_email_carries_the_reset_link() {
        let mail = password_reset_email(&site(None), "tok456");
        assert!(mail.text.contains("https://flash.example.com/reset/tok456"));
        assert!(mail.html.contains("https://flash.example.com/reset/tok456"));
    }

    #[test]
    fn html_escape_covers_all_five() {
        assert_eq!(
            html_escape("it's \"x\" <b>&</b>"),
            "it&#39;s &quot;x&quot; &lt;b&gt;&amp;&lt;/b&gt;"
        );
    }
}
