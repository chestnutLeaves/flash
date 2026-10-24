//! Flash's panel: a view MCP hosts render inside the conversation (the
//! MCP Apps extension, `io.modelcontextprotocol/ui`). One page with two
//! views: studying (`start_study_session`: the card's front, the back on
//! request, one button per grade, the session's progress) and the cards
//! just made or edited (`create_cards`, `update_cards`: one card at a
//! time, an edit's old text beside the new). It holds no credentials:
//! every action is a tool call the host proxies over the user's own
//! connection, and card media loads through links signed here, since the
//! host's sandbox frame carries no Bearer token.
//!
//! What lives here is the protocol surface: the page, its resource and
//! its metadata, the per-tool visibility, and the signed media links.
//! The tools themselves are in `mcp.rs`.

use std::sync::Arc;

use flash_core::{MediaId, UserId};
use hmac::{Hmac, KeyInit, Mac};
use rmcp::model::{MetaObject, ReadResourceResult, Resource, ResourceContents};
use rmcp::service::{RequestContext, RoleServer};
use serde_json::{json, Value};
use sha2::Sha256;

/// The extension a host names in its `initialize` capabilities when it
/// can render the panel.
pub const EXTENSION: &str = "io.modelcontextprotocol/ui";

/// The media type of an MCP Apps view.
pub const MIME: &str = "text/html;profile=mcp-app";

/// The panel's resource. Hosts cache a view by its URI, so the version in
/// the name goes up with every change to the page.
pub const URI: &str = "ui://flash/panel-v34.html";

/// URIs the panel had before, still answered with today's page. A host
/// may keep the tool list it was given, and the page that list names,
/// for a long time. Only `URI` is listed.
pub const EARLIER_URIS: &[&str] = &[
    "ui://flash/panel-v33.html",
    "ui://flash/panel-v32.html",
    "ui://flash/panel-v31.html",
    "ui://flash/panel-v30.html",
    "ui://flash/panel-v29.html",
    "ui://flash/panel-v28.html",
    "ui://flash/panel-v27.html",
    "ui://flash/panel-v26.html",
    "ui://flash/panel-v25.html",
    "ui://flash/panel-v24.html",
    "ui://flash/panel-v23.html",
    "ui://flash/panel-v22.html",
    "ui://flash/panel-v21.html",
    "ui://flash/panel-v20.html",
    "ui://flash/panel-v19.html",
    "ui://flash/panel-v18.html",
    "ui://flash/panel-v17.html",
    "ui://flash/panel-v16.html",
    "ui://flash/panel-v15.html",
    "ui://flash/panel-v14.html",
    "ui://flash/panel-v13.html",
    "ui://flash/panel-v12.html",
    "ui://flash/panel-v11.html",
    "ui://flash/panel-v10.html",
    "ui://flash/panel-v9.html",
    "ui://flash/panel-v8.html",
    "ui://flash/panel-v7.html",
    "ui://flash/panel-v6.html",
    "ui://flash/panel-v5.html",
    "ui://flash/panel-v4.html",
    "ui://flash/panel-v3.html",
    "ui://flash/study-panel-v2.html",
];

/// Where a tool's result carries the panel's own data, in `_meta`: hosts
/// hand `_meta` to the view and keep it out of the model's context.
pub const PAYLOAD: &str = "flash/panel";

/// The page's markup and its script, two files so the script can be
/// checked as a script. Hosts get one page, the script inline: a sandboxed
/// view loads nothing else from here.
const MARKUP: &str = include_str!("../../panel/panel.html");
const SCRIPT: &str = include_str!("../../panel/panel.js");
const SCRIPT_MARK: &str = "__FLASH_PANEL_SCRIPT__";
/// Inline like the panel's script: a change to static/i18n.js is a change
/// to the page, so it moves `URI` too.
const I18N: &str = include_str!("../../static/i18n.js");
const I18N_MARK: &str = "__FLASH_I18N_SCRIPT__";
const NEGOTIATE: &str = include_str!("../../panel/negotiate.js");
const NEGOTIATE_MARK: &str = "__FLASH_NEGOTIATE_SCRIPT__";
const PAYLOAD_SCRIPT: &str = include_str!("../../panel/payload.js");
const PAYLOAD_MARK: &str = "__FLASH_PAYLOAD_SCRIPT__";
const LANGUAGES_MARK: &str = "__FLASH_PANEL_LANGUAGES__";
const WORDS_MARK: &str = "__FLASH_PANEL_WORDS__";

fn page() -> &'static str {
    static PAGE: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        MARKUP
            .replacen(LANGUAGES_MARK, &languages(), 1)
            .replacen(NEGOTIATE_MARK, &without_comment_lines(NEGOTIATE), 1)
            .replacen(I18N_MARK, &without_comment_lines(I18N), 1)
            .replacen(PAYLOAD_MARK, &without_comment_lines(PAYLOAD_SCRIPT), 1)
            .replacen(SCRIPT_MARK, &without_comment_lines(SCRIPT), 1)
    });
    &PAGE
}

/// A script without the lines that are only a comment: about two fifths
/// of the panel's scripts, which every host would load before the panel
/// shows. A line goes only when the comment is the whole of it: a `//`
/// line, or a `/* ... */` that starts a line and ends one, alone. A block
/// that ends with code after it stays whole. A test parses the page's
/// scripts and their files and holds them equal, token for token.
pub fn without_comment_lines(script: &str) -> String {
    let mut out = String::with_capacity(script.len());
    let mut block: Option<Vec<&str>> = None;
    for line in script.split_inclusive('\n') {
        let text = line.trim_start();
        if let Some(held) = block.as_mut() {
            held.push(line);
            if let Some(end) = text.find("*/") {
                if !text[end + 2..].trim().is_empty() {
                    out.extend(held.iter().copied());
                }
                block = None;
            }
            continue;
        }
        if text.starts_with("//") {
            continue;
        }
        if let Some(rest) = text.strip_prefix("/*") {
            match rest.find("*/") {
                Some(end) if rest[end + 2..].trim().is_empty() => {}
                Some(_) => out.push_str(line),
                None => block = Some(vec![line]),
            }
            continue;
        }
        out.push_str(line);
    }
    out.extend(block.unwrap_or_default());
    out
}

fn languages() -> String {
    crate::i18n::island(
        "languages",
        &json!({ "languages": flash_core::locale::LANGUAGES }),
    )
}

/// The panel's words (`panel-*`, and the grade buttons' `interval-*`) in
/// every language Flash speaks: a host caches one page for everyone, and
/// which language it reads in is known only once the account and the
/// host have said (panel.js sets the page's `lang`, and static/i18n.js
/// reads it). A header per language names the folders it reads, and each
/// folder's own messages are carried once, whatever number of languages
/// read them (English's and the Spanish base's under both Spanishes).
fn words(catalog: &Arc<crate::i18n::Catalog>) -> String {
    const PREFIXES: &[&str] = &["panel-", "interval-"];
    let mut out = String::new();
    let mut folders: Vec<&'static str> = Vec::new();
    for &locale in catalog.shipped() {
        out.push_str(&crate::i18n::Tr::new(catalog.clone(), locale, None).script_header());
        for folder in crate::i18n::reading_order(locale) {
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
    }
    for folder in folders {
        let messages = catalog.folder_table(folder, PREFIXES);
        if !messages.is_empty() {
            out.push_str(&crate::i18n::folder_island(folder, messages));
        }
    }
    out
}

/// Whether `uri` names the panel, today's or an earlier one.
pub fn is_panel(uri: &str) -> bool {
    uri == URI || EARLIER_URIS.contains(&uri)
}

fn meta(value: Value) -> MetaObject {
    match value {
        Value::Object(map) => MetaObject(map),
        _ => unreachable!("tool metadata is always an object"),
    }
}

/// Who may call a tool: the model, the panel, or both. A host refuses a
/// panel's call to a tool without "app" and keeps a tool without "model"
/// out of the model's list. `openai/widgetAccessible` is the older
/// spelling of "the panel may call it", kept for hosts that read only
/// that.
fn visibility(model: bool, app: bool) -> Value {
    let mut who = Vec::new();
    if model {
        who.push("model");
    }
    if app {
        who.push("app");
    }
    json!({
        "ui": { "visibility": who },
        "openai/widgetAccessible": app,
    })
}

/// A tool only the model calls: every tool the panel has no use for, so a
/// card's content can never reach one through the panel.
pub fn for_model() -> MetaObject {
    meta(visibility(true, false))
}

/// A tool both the model and the panel call.
pub fn for_both() -> MetaObject {
    meta(visibility(true, true))
}

/// A tool only the panel calls; hosts leave it out of the model's list.
pub fn for_panel() -> MetaObject {
    meta(visibility(false, true))
}

/// A tool whose result opens the panel. `ui/resourceUri` and
/// `openai/outputTemplate` are the older spellings of the same link (the
/// MCP Apps library still writes the first beside `ui.resourceUri`), and
/// the `openai/toolInvocation` keys are the host's status line while the
/// call runs and once it is done.
fn linked(mut value: Value, invoking: &str, invoked: &str) -> MetaObject {
    value["ui"]["resourceUri"] = json!(URI);
    value["ui/resourceUri"] = json!(URI);
    value["openai/outputTemplate"] = json!(URI);
    value["openai/toolInvocation/invoking"] = json!(invoking);
    value["openai/toolInvocation/invoked"] = json!(invoked);
    meta(value)
}

/// `start_study_session`: callable by both, since the panel starts a
/// session again, and its result opens the panel.
pub fn opens_panel() -> MetaObject {
    linked(visibility(true, true), "Opening your cards", "Cards ready")
}

/// A tool only the model calls whose result the panel shows: the cards
/// just made or edited. The panel can never call it.
pub fn shows_panel(invoking: &str, invoked: &str) -> MetaObject {
    linked(visibility(true, false), invoking, invoked)
}

/// What the page may load: images and sound from this server's signed
/// media links, nothing fetched, nothing framed.
///
/// The page's own origin goes in `openai/widgetDomain` and never in
/// `ui.domain`: a host that reads the two as one setting wants the
/// page's `https://` origin there, while a host that reads only
/// `ui.domain` expects a sandbox origin of its own there and draws
/// nothing on any other value.
fn csp(base_url: &str) -> Value {
    let origin = base_url.trim_end_matches('/');
    json!({
        "ui": {
            "csp": { "connectDomains": [], "resourceDomains": [origin] },
            "prefersBorder": true,
        },
        "openai/widgetDomain": origin,
        "openai/widgetCSP": { "connect_domains": [], "resource_domains": [origin] },
        "openai/widgetPrefersBorder": true,
        "openai/widgetDescription": "The user's flashcards, one at a time: a study session with a button for each grade, or the cards just made or edited.",
    })
}

/// The panel as `resources/list` names it.
pub fn resource(base_url: &str) -> Resource {
    Resource::new(URI, "panel")
        .with_title("Flash panel")
        .with_description(
            "Study the user's due cards, revealing and grading each; or page through the cards just made or edited.",
        )
        .with_mime_type(MIME)
        .with_meta(meta(csp(base_url)))
}

/// Where the page names this server, for the site's own font, which it
/// loads from here like the card media.
const ORIGIN_MARK: &str = "__FLASH_ORIGIN__";

pub fn page_text(base_url: &str, catalog: &Arc<crate::i18n::Catalog>) -> String {
    page()
        .replace(ORIGIN_MARK, base_url.trim_end_matches('/'))
        .replacen(WORDS_MARK, &words(catalog), 1)
}

/// The page itself, for `resources/read` of `uri` (today's or an earlier
/// one; see `is_panel`), as a server at `base_url` giving readers the
/// languages of `catalog` serves it.
pub fn read(base_url: &str, uri: &str, catalog: &Arc<crate::i18n::Catalog>) -> ReadResourceResult {
    let page = page_text(base_url, catalog);
    ReadResourceResult::new(vec![ResourceContents::text(page, uri)
        .with_mime_type(MIME)
        .with_meta(meta(csp(base_url)))])
}

/// Whether the host renders MCP Apps views. A client on MCP 2026-07-28
/// names its capabilities in each request's `_meta` (claude.ai does,
/// and its connection's own handshake lists none); an older one names
/// them once, in `initialize`. ChatGPT, Claude and Grok all show the
/// view (Grok below its reply, a moment after it). Mistral's Le Chat shows
/// it too without naming the extension, so this answers false there;
/// Gemini neither names it nor reads the page (checked 2026-10-11).
pub fn host_shows_panel(ctx: &RequestContext<RoleServer>) -> bool {
    let names_panel = |caps: &rmcp::model::ClientCapabilities| {
        caps.extensions
            .as_ref()
            .is_some_and(|ext| ext.contains_key(EXTENSION))
    };
    ctx.meta
        .client_capabilities()
        .is_some_and(|caps| names_panel(&caps))
        || ctx
            .peer
            .peer_info()
            .is_some_and(|info| names_panel(&info.capabilities))
}

/// The cache hints (SEP-2549) a client on MCP 2026-07-28 requires on a
/// list or read result, as the generated `tools/list` gives them: fresh
/// on every read, the same for everyone. None for an older client.
pub fn cache_hints(ctx: &RequestContext<RoleServer>) -> Option<(u64, rmcp::model::CacheScope)> {
    ctx.protocol_version()
        .is_some_and(|v| v >= rmcp::model::ProtocolVersion::V_2026_07_28)
        .then_some((0, rmcp::model::CacheScope::Public))
}

type HmacSha256 = Hmac<Sha256>;

/// Media links the panel can load without credentials: the media id, its
/// owner and an expiry, signed with the deployment's key. The key is
/// drawn once and kept in the database (`server_state`), so links in
/// panels already open keep working across a restart, and there is no
/// secret to configure.
#[derive(Clone)]
pub struct MediaLinks {
    key: Arc<[u8; 32]>,
}

impl MediaLinks {
    /// How long a link works: the rest of a study sitting, and a panel
    /// scrolled back to later the same day.
    pub const TTL_MS: i64 = 24 * 60 * 60 * 1000;

    /// The deployment's key, drawn on first use and kept from then on.
    pub fn stored(store: &flash_store::Store) -> flash_store::Result<Self> {
        use base64::Engine;
        const NAME: &str = "panel_media_key";
        let fresh = base64::engine::general_purpose::STANDARD.encode(rand::random::<[u8; 32]>());
        let kept = store.server_state_or_insert(NAME, &fresh)?;
        let key: [u8; 32] = base64::engine::general_purpose::STANDARD
            .decode(kept)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or_else(|| flash_store::StoreError::Invalid(format!("{NAME} is not a key")))?;
        Ok(Self { key: Arc::new(key) })
    }

    fn mac(&self, owner: UserId, media: MediaId, expires_ms: i64) -> HmacSha256 {
        let mut mac =
            HmacSha256::new_from_slice(self.key.as_slice()).expect("hmac accepts any key length");
        mac.update(format!("{}:{}:{expires_ms}", owner.raw(), media.0).as_bytes());
        mac
    }

    /// An absolute link to one of `owner`'s media, good for `TTL_MS`.
    pub fn url(&self, base_url: &str, owner: UserId, media: MediaId, now_ms: i64) -> String {
        use base64::Engine;
        let expires = now_ms + Self::TTL_MS;
        let sig = self.mac(owner, media, expires).finalize().into_bytes();
        format!(
            "{}/mcp-media/{}/s?e={expires}&s={}",
            base_url.trim_end_matches('/'),
            media.0,
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sig)
        )
    }

    /// Whether `sig` is the deployment's signature for `owner`'s `media`
    /// until `expires_ms`, and that moment is still ahead (and no further
    /// than a link is ever made for).
    pub fn verify(
        &self,
        owner: UserId,
        media: MediaId,
        expires_ms: i64,
        sig: &str,
        now_ms: i64,
    ) -> bool {
        use base64::Engine;
        if expires_ms <= now_ms || expires_ms > now_ms + Self::TTL_MS {
            return false;
        }
        let Ok(sig) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(sig) else {
            return false;
        };
        self.mac(owner, media, expires_ms)
            .verify_slice(&sig)
            .is_ok()
    }

    /// Card HTML with each of `owner`'s `/media/{id}` references turned
    /// into a signed absolute link. Only the panel's copies are rewritten;
    /// what the model reads keeps the plain references.
    pub fn sign_html(&self, html: &str, base_url: &str, owner: UserId, now_ms: i64) -> String {
        let mut out = html.to_string();
        for id in flash_store::richtext::media_ids(html) {
            // The link's `&` is escaped: it lands inside an attribute.
            let url = self
                .url(base_url, owner, MediaId(id), now_ms)
                .replace('&', "&amp;");
            out = out.replace(&format!("src=\"/media/{id}\""), &format!("src=\"{url}\""));
        }
        out
    }

    /// `sign_html` undone, for card HTML the panel sends back (its quick
    /// edit): each `src` pointing at this server's `/mcp-media/{id}`, as
    /// a signed or a bearer link, becomes `/media/{id}` again, the only
    /// form a card may keep. Whose media it names is the save's to check,
    /// as for any edit. Anything else is left for the sanitizer.
    pub fn unsign_html(html: &str, base_url: &str) -> String {
        let prefix = format!("src=\"{}/mcp-media/", base_url.trim_end_matches('/'));
        let mut out = String::with_capacity(html.len());
        let mut rest = html;
        while let Some(at) = rest.find(&prefix) {
            out.push_str(&rest[..at]);
            let after = &rest[at + prefix.len()..];
            let digits = after.chars().take_while(char::is_ascii_digit).count();
            let end = after.find('"');
            match end {
                Some(end)
                    if digits > 0 && matches!(after.as_bytes().get(digits), Some(b'"' | b'/')) =>
                {
                    out.push_str(&format!("src=\"/media/{}\"", &after[..digits]));
                    rest = &after[end + 1..];
                }
                _ => {
                    out.push_str(&prefix);
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }
}
